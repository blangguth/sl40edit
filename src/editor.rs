// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Benjamin Langguth

//! Channel ordering with grab-and-move, move-to-position and undo/redo.
//!
//! The editor never touches the file. It only manipulates `order`, a list of
//! indices into `DbmFile::channels` where position `n` becomes program number `n`.
//! Positions are 0-based. The UI shows them 1-based.

#[derive(Debug, Clone)]
struct Step {
    before: Vec<usize>,
    after: Vec<usize>,
    cursor_before: usize,
    cursor_after: usize,
}

#[derive(Debug, Clone)]
struct Grab {
    snapshot: Vec<usize>,
    from: usize,
    pos: usize,
}

#[derive(Debug, Clone)]
pub struct Editor {
    order: Vec<usize>,
    saved: Vec<usize>,
    undo: Vec<Step>,
    redo: Vec<Step>,
    grab: Option<Grab>,
}

impl Editor {
    pub fn new(order: Vec<usize>) -> Self {
        Self {
            saved: order.clone(),
            order,
            undo: Vec::new(),
            redo: Vec::new(),
            grab: None,
        }
    }

    pub fn order(&self) -> &[usize] {
        &self.order
    }

    pub fn len(&self) -> usize {
        self.order.len()
    }

    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    /// True if the order differs from the last loaded or saved state.
    pub fn is_dirty(&self) -> bool {
        self.order != self.saved
    }

    /// Call after a successful save.
    pub fn mark_saved(&mut self) {
        self.saved = self.order.clone();
    }

    pub fn undo_depth(&self) -> usize {
        self.undo.len()
    }

    /// Current position of the grabbed channel, if a grab is active.
    pub fn grabbed(&self) -> Option<usize> {
        self.grab.as_ref().map(|g| g.pos)
    }

    /// Start moving the channel at `pos`. No-op if a grab is already active or
    /// `pos` is out of range.
    pub fn grab(&mut self, pos: usize) {
        if self.grab.is_none() && pos < self.order.len() {
            self.grab = Some(Grab {
                snapshot: self.order.clone(),
                from: pos,
                pos,
            });
        }
    }

    /// Move the grabbed channel to `to` (clamped), shifting the others.
    /// Returns its new position, or `None` if nothing is grabbed.
    pub fn move_grabbed(&mut self, to: usize) -> Option<usize> {
        let last = self.order.len().checked_sub(1)?;
        let grab = self.grab.as_mut()?;
        let to = to.min(last);
        move_item(&mut self.order, grab.pos, to);
        grab.pos = to;
        Some(to)
    }

    /// Finish the grab. The whole grab becomes one undo step if it changed
    /// anything. Returns the channel's final position.
    pub fn drop_grab(&mut self) -> Option<usize> {
        let grab = self.grab.take()?;
        if self.order != grab.snapshot {
            self.push(Step {
                before: grab.snapshot,
                after: self.order.clone(),
                cursor_before: grab.from,
                cursor_after: grab.pos,
            });
        }
        Some(grab.pos)
    }

    /// Abort the grab and restore the order from before it. Returns the
    /// channel's original position.
    pub fn cancel_grab(&mut self) -> Option<usize> {
        let grab = self.grab.take()?;
        self.order = grab.snapshot;
        Some(grab.from)
    }

    /// Move the channel at `from` to `to` (clamped) as one undo step.
    /// Returns the new position. Ignored while a grab is active.
    pub fn move_to(&mut self, from: usize, to: usize) -> Option<usize> {
        if self.grab.is_some() || from >= self.order.len() {
            return None;
        }
        let to = to.min(self.order.len() - 1);
        if from != to {
            let before = self.order.clone();
            move_item(&mut self.order, from, to);
            self.push(Step {
                before,
                after: self.order.clone(),
                cursor_before: from,
                cursor_after: to,
            });
        }
        Some(to)
    }

    /// Returns the cursor position to restore, or `None` if there's nothing to undo.
    pub fn undo(&mut self) -> Option<usize> {
        if self.grab.is_some() {
            return None;
        }
        let step = self.undo.pop()?;
        self.order = step.before.clone();
        let cursor = step.cursor_before;
        self.redo.push(step);
        Some(cursor)
    }

    /// Returns the cursor position to restore, or `None` if there's nothing to redo.
    pub fn redo(&mut self) -> Option<usize> {
        if self.grab.is_some() {
            return None;
        }
        let step = self.redo.pop()?;
        self.order = step.after.clone();
        let cursor = step.cursor_after;
        self.undo.push(step);
        Some(cursor)
    }

    fn push(&mut self, step: Step) {
        self.undo.push(step);
        self.redo.clear();
    }
}

fn move_item(v: &mut Vec<usize>, from: usize, to: usize) {
    let item = v.remove(from);
    v.insert(to, item);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ed() -> Editor {
        Editor::new(vec![10, 11, 12, 13, 14])
    }

    #[test]
    fn move_to_forward_and_backward() {
        let mut e = ed();
        assert_eq!(e.move_to(0, 3), Some(3));
        assert_eq!(e.order(), [11, 12, 13, 10, 14]);
        assert_eq!(e.move_to(4, 0), Some(0));
        assert_eq!(e.order(), [14, 11, 12, 13, 10]);
        assert_eq!(e.undo_depth(), 2);
    }

    #[test]
    fn move_to_clamps_and_ignores_noop() {
        let mut e = ed();
        assert_eq!(e.move_to(1, 99), Some(4));
        assert_eq!(e.order(), [10, 12, 13, 14, 11]);
        assert_eq!(e.move_to(2, 2), Some(2));
        assert_eq!(e.undo_depth(), 1);
        assert_eq!(e.move_to(9, 0), None);
    }

    #[test]
    fn grab_move_drop_is_one_undo_step() {
        let mut e = ed();
        e.grab(1);
        assert_eq!(e.grabbed(), Some(1));
        assert_eq!(e.move_grabbed(2), Some(2));
        assert_eq!(e.move_grabbed(3), Some(3));
        assert_eq!(e.move_grabbed(100), Some(4));
        assert_eq!(e.order(), [10, 12, 13, 14, 11]);
        assert_eq!(e.drop_grab(), Some(4));
        assert_eq!(e.grabbed(), None);
        assert_eq!(e.undo_depth(), 1);

        assert_eq!(e.undo(), Some(1));
        assert_eq!(e.order(), [10, 11, 12, 13, 14]);
        assert_eq!(e.redo(), Some(4));
        assert_eq!(e.order(), [10, 12, 13, 14, 11]);
    }

    #[test]
    fn grab_back_to_start_adds_no_step() {
        let mut e = ed();
        e.grab(2);
        e.move_grabbed(0);
        e.move_grabbed(2);
        e.drop_grab();
        assert_eq!(e.undo_depth(), 0);
        assert!(!e.is_dirty());
    }

    #[test]
    fn cancel_restores_order() {
        let mut e = ed();
        e.grab(0);
        e.move_grabbed(4);
        assert_eq!(e.cancel_grab(), Some(0));
        assert_eq!(e.order(), [10, 11, 12, 13, 14]);
        assert_eq!(e.undo_depth(), 0);
    }

    #[test]
    fn no_edits_or_undo_while_grabbing() {
        let mut e = ed();
        e.move_to(0, 1);
        e.grab(3);
        assert_eq!(e.move_to(0, 4), None);
        assert_eq!(e.undo(), None);
        assert_eq!(e.redo(), None);
        e.grab(0); // second grab ignored
        assert_eq!(e.grabbed(), Some(3));
    }

    #[test]
    fn new_edit_clears_redo() {
        let mut e = ed();
        e.move_to(0, 1);
        e.undo();
        e.move_to(4, 0);
        assert_eq!(e.redo(), None);
    }

    #[test]
    fn dirty_tracking() {
        let mut e = ed();
        assert!(!e.is_dirty());
        e.move_to(0, 1);
        assert!(e.is_dirty());
        e.undo();
        assert!(!e.is_dirty());
        e.redo();
        e.mark_saved();
        assert!(!e.is_dirty());
        e.undo();
        assert!(e.is_dirty());
    }
}
