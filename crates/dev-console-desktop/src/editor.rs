//! The editor's draft state: the entry being edited, the text typed over it, and the saved record
//! both are compared against.
//!
//! This module exists to name the one gap in the native shell that made five shipped behaviours
//! unexpressible, because the fix is a *type*, not a screen, and a type that quietly widens a
//! placeholder into "the draft" is worth reading before it is used elsewhere.

/// The editor's draft, and the saved record it is a copy of.
///
/// ## Why the *saved* record is part of this module
///
/// A draft is only "unsaved" relative to something, and in the shell as it stands the only
/// "something" was the same string the row already displays - so "the draft differs from the saved
/// record" was unanswerable, and with it the shipped app's `draftDirty` (`JSON.stringify(draft) !==
/// baseline` in `src/windows/Library.svelte`), the `mode === 'edit' && draftDirty` guard that refuses
/// to switch tabs, to re-select, to pin or to close while there is something to lose, and the
/// unsaved-changes prompt that answers it. Those are five behaviours, and the dirty chip the shipped
/// editor draws beside its title as `data-testid="dirty-indicator"`, that no native type could express
/// while the saved entry was the same string as the draft.
///
/// [`EntryDraft`] is the smallest thing that closes that gap: the two fields the placeholder list and
/// the placeholder editor can actually draw - a name and command text - and nothing else. It is
/// **not** the full `EntryDraft` of `src/lib/models.ts`, and claiming that shape is the one thing to
/// avoid reading into it: no id, no `Kind`, revision, `customType`, tags, description, notes, ordered
/// custom fields, working directory or `pin` bit, and no secret *value* - the `SecretDraft` half of
/// that type stays absent, and the capability matrix says which behaviours wait on it. What it buys is
/// exactly one thing: a place for the *saved* text to live that is not the draft.
///
/// [`Editor`] is the same gap's second half: the draft, a snapshot of the record it was copied from,
/// and where it came from. Holding the snapshot rather than re-reading `Library::entries` is
/// deliberate, and it is how the shipped code behaves: `baseline` is taken when the editor opens, so
/// reverting the draft to the saved text makes "unsaved" go away and an unrelated edit to the stored
/// record elsewhere cannot silently make a dirty draft look clean.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryDraft {
    /// The entry's name - what the row shows and what the placeholder editor edits. Metadata, never a
    /// value.
    pub name: String,
    /// The saved command text. Part of the saved record so a write-back cannot blank it; the native
    /// editor has no field for it yet, which is why [`Editor`]'s draft carries it unchanged rather
    /// than editing it.
    pub command: String,
}

/// An open editor: what is being edited, the draft typed over it, and the text it was copied from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Editor {
    /// Which row the editor was opened on. An index rather than the record itself, because the record
    /// lives in the list and a second copy of it here would be a second thing to keep in step.
    pub index: usize,
    /// The draft: what the fields hold right now, and the only copy of the changes until they are
    /// saved. Held as *values*, not as Rune text widgets, because the shipped draft is plain state too
    /// and an editor that has to be on screen to know what it holds is not the same thing.
    pub draft: EntryDraft,
    /// The text the draft was copied from - the shipped `baseline`, taken when the editor opened and
    /// never re-derived.
    pub baseline: EntryDraft,
}

impl Editor {
    /// Opens an editor over `record`, its draft a copy of it. The name carries over and so does the
    /// command text, which the placeholder editor does not edit and must not lose.
    pub fn new(index: usize, record: &EntryDraft) -> Editor {
        Editor {
            index,
            draft: record.clone(),
            baseline: record.clone(),
        }
    }
}
