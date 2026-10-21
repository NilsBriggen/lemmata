//! Files and `lemmata://` links macOS hands over as Apple Events (Tauri's `RunEvent::Opened`).
//! winit has no hook for them, so the handlers go in as AppKit finishes launching, after its own
//! defaults, as Apple's docs say. Whatever arrives before the app exists waits in `QUEUE`.

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject, NSObjectProtocol};
use objc2::{ClassType, define_class, msg_send, sel};
use objc2_foundation::{
    NSAppleEventDescriptor, NSAppleEventManager, NSNotification, NSNotificationCenter, NSString,
};
use std::sync::Mutex;

type Sink = Box<dyn Fn(Vec<url::Url>) + Send>;

/// Opened before the app was ready, and then where they go.
static QUEUE: Mutex<(Vec<url::Url>, Option<Sink>)> = Mutex::new((Vec::new(), None));

const fn code(c: &[u8; 4]) -> u32 {
    u32::from_be_bytes(*c)
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements, and this class has no Drop.
    #[unsafe(super(NSObject))]
    #[name = "LemmataAppleEvents"]
    struct Handler;

    impl Handler {
        #[unsafe(method(handleEvent:withReplyEvent:))]
        fn handle_event(&self, event: &NSAppleEventDescriptor, _reply: &NSAppleEventDescriptor) {
            deliver(urls(event));
        }

        #[unsafe(method(willFinishLaunching:))]
        fn will_finish_launching(&self, _: &NSNotification) {
            let m = NSAppleEventManager::sharedAppleEventManager();
            // Links (kInternetEventClass, kAEGetURL) and files (kCoreEventClass, kAEOpenDocuments).
            for (class, id) in [(b"GURL", b"GURL"), (b"aevt", b"odoc")] {
                let _: () = unsafe {
                    msg_send![&*m, setEventHandler: self, andSelector: sel!(handleEvent:withReplyEvent:), forEventClass: code(class), andEventID: code(id)]
                };
            }
        }
    }

    unsafe impl NSObjectProtocol for Handler {}
);

/// The event's direct object: one link, or a list of files.
fn urls(event: &NSAppleEventDescriptor) -> Vec<url::Url> {
    let direct: Option<Retained<NSAppleEventDescriptor>> =
        unsafe { msg_send![event, paramDescriptorForKeyword: code(b"----")] };
    let Some(direct) = direct else {
        return Vec::new();
    };
    let one = |d: &NSAppleEventDescriptor| {
        let text = match d.fileURLValue() {
            Some(u) => u.absoluteString(),
            None => d.stringValue(),
        };
        text.and_then(|s| url::Url::parse(&s.to_string()).ok())
    };
    match direct.numberOfItems() {
        n if n > 0 => (1..=n)
            .filter_map(|i| direct.descriptorAtIndex(i))
            .filter_map(|d| one(&d))
            .collect(),
        _ => one(&direct).into_iter().collect(),
    }
}

fn deliver(urls: Vec<url::Url>) {
    if urls.is_empty() {
        return;
    }
    let mut q = QUEUE.lock().unwrap_or_else(|e| e.into_inner());
    match &q.1 {
        Some(sink) => sink(urls),
        None => q.0.extend(urls),
    }
}

/// Call before the event loop starts.
pub fn listen() {
    let handler: Retained<Handler> = unsafe { msg_send![Handler::class(), new] };
    let name = NSString::from_str("NSApplicationWillFinishLaunchingNotification");
    unsafe {
        NSNotificationCenter::defaultCenter().addObserver_selector_name_object(
            &handler,
            sel!(willFinishLaunching:),
            Some(&name),
            None::<&AnyObject>,
        );
    }
    // Both hold it weakly: it lives as long as the process.
    std::mem::forget(handler);
}

/// Where opened files and links go once the app is running; earlier ones go there at once.
pub fn on_opened(sink: impl Fn(Vec<url::Url>) + Send + 'static) {
    let mut q = QUEUE.lock().unwrap_or_else(|e| e.into_inner());
    let waiting = std::mem::take(&mut q.0);
    if !waiting.is_empty() {
        sink(waiting);
    }
    q.1 = Some(Box::new(sink));
}
