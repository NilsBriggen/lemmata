// A snippet that uses one field twice (\begin{${name}} … \end{${name}}) gives one cursor
// per copy, so the name is typed into both. CodeMirror ends the snippet when the cursor
// leaves its fields (an arrow key, Tab past the last field, Esc), but leaves those extra
// cursors behind, and further typing lands in two places. This drops them as the snippet ends.
import { hasNextSnippetField, hasPrevSnippetField } from "@codemirror/autocomplete";
import { EditorSelection, EditorState } from "@codemirror/state";

const inSnippet = (s: EditorState) => hasNextSnippetField(s) || hasPrevSnippetField(s);

export const endSnippetCursors = EditorState.transactionFilter.of((tr) => {
  // Only the transaction that ends a snippet with several cursors; cursors a user adds stay.
  if (tr.startState.selection.ranges.length < 2 || !inSnippet(tr.startState)) return tr;
  const after = tr.state;
  if (inSnippet(after) || after.selection.ranges.length < 2) return tr;
  return [tr, { selection: EditorSelection.single(after.selection.main.anchor, after.selection.main.head), sequential: true }];
});
