//! A generational arena: the storage backbone of the retained element tree.
//!
//! # Why a generational arena?
//!
//! The retained tree (one `Element` per mounted view) needs three things at
//! once that the borrow checker makes awkward with `Box`/`Rc` graphs:
//!
//! 1. **Stable handles** that parents/children/the runtime can hold without
//!    borrowing the tree.
//! 2. **O(1) access** from a handle to its node.
//! 3. **Safe slot reuse** — when a node is removed, its slot is recycled, but a
//!    stale handle to the *old* occupant must not silently resolve to the *new*
//!    one. This "ABA" bug is the classic footgun of index-based trees.
//!
//! Each slot carries a `generation` counter, bumped on every removal. An
//! [`Index`] records the generation it was minted at, so a stale handle fails
//! the generation check and resolves to `None` instead of aliasing fresh data.
//!
//! We hand-roll this (rather than depend on `slotmap`) because it is genuinely
//! core, must stay `no_std`, and is small enough to own and test exhaustively.

use alloc::vec::Vec;

/// A stable, generation-checked handle into an [`Arena`].
///
/// `Index` is `Copy` and carries no lifetime, so it can be freely stored in
/// node structs, message queues, and the mutation list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Index {
    slot: u32,
    generation: u32,
}

impl Index {
    /// The raw slot number. Exposed for debugging/inspector tooling only;
    /// prefer passing the whole `Index` around.
    pub fn slot(self) -> u32 {
        self.slot
    }

    /// The generation this handle was minted at.
    pub fn generation(self) -> u32 {
        self.generation
    }

    /// Reconstructs an index from its raw parts. Intended for round-tripping a
    /// handle through an opaque integer (e.g. a native view's `tag`); the result
    /// is only valid if the parts came from a live [`Index`].
    pub const fn from_raw(slot: u32, generation: u32) -> Index {
        Index { slot, generation }
    }
}

/// What a slot currently holds.
enum Entry<T> {
    /// Live value.
    Occupied(T),
    /// Free slot; points at the next free slot in the free list (`u32::MAX`
    /// sentinel means "end of list").
    Free(u32),
}

struct Slot<T> {
    generation: u32,
    entry: Entry<T>,
}

const NIL: u32 = u32::MAX;

/// A generational arena storing values of type `T`.
pub struct Arena<T> {
    slots: Vec<Slot<T>>,
    /// Head of the intrusive free list, or [`NIL`] if there are no free slots.
    free_head: u32,
    /// Count of currently occupied slots.
    len: u32,
}

impl<T> Default for Arena<T> {
    fn default() -> Self {
        Arena::new()
    }
}

impl<T> Arena<T> {
    /// Creates an empty arena.
    pub const fn new() -> Self {
        Arena {
            slots: Vec::new(),
            free_head: NIL,
            len: 0,
        }
    }

    /// Creates an empty arena with space preallocated for `capacity` slots.
    pub fn with_capacity(capacity: usize) -> Self {
        Arena {
            slots: Vec::with_capacity(capacity),
            free_head: NIL,
            len: 0,
        }
    }

    /// Number of live values.
    pub fn len(&self) -> usize {
        self.len as usize
    }

    /// Whether the arena holds no live values.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Inserts `value`, returning a handle to it.
    ///
    /// Reuses a freed slot when one is available (O(1)), otherwise grows the
    /// backing vector.
    pub fn insert(&mut self, value: T) -> Index {
        self.len += 1;
        if self.free_head != NIL {
            // Pop the free list head and occupy it.
            let slot_idx = self.free_head;
            let slot = &mut self.slots[slot_idx as usize];
            self.free_head = match slot.entry {
                Entry::Free(next) => next,
                Entry::Occupied(_) => unreachable!("free list pointed at an occupied slot"),
            };
            slot.entry = Entry::Occupied(value);
            Index {
                slot: slot_idx,
                generation: slot.generation,
            }
        } else {
            // Append a brand-new slot at generation 0.
            let slot_idx = self.slots.len() as u32;
            self.slots.push(Slot {
                generation: 0,
                entry: Entry::Occupied(value),
            });
            Index {
                slot: slot_idx,
                generation: 0,
            }
        }
    }

    /// Returns `true` if `index` resolves to a live value (matching slot,
    /// matching generation, currently occupied).
    pub fn contains(&self, index: Index) -> bool {
        self.get(index).is_some()
    }

    /// Borrows the value at `index`, or `None` if the handle is stale or freed.
    pub fn get(&self, index: Index) -> Option<&T> {
        match self.slots.get(index.slot as usize) {
            Some(slot) if slot.generation == index.generation => match &slot.entry {
                Entry::Occupied(value) => Some(value),
                Entry::Free(_) => None,
            },
            _ => None,
        }
    }

    /// Mutably borrows the value at `index`, or `None` if stale or freed.
    pub fn get_mut(&mut self, index: Index) -> Option<&mut T> {
        match self.slots.get_mut(index.slot as usize) {
            Some(slot) if slot.generation == index.generation => match &mut slot.entry {
                Entry::Occupied(value) => Some(value),
                Entry::Free(_) => None,
            },
            _ => None,
        }
    }

    /// Removes and returns the value at `index`, recycling the slot.
    ///
    /// Returns `None` (and changes nothing) if the handle is already stale or
    /// freed, so double-removes are safe no-ops.
    pub fn remove(&mut self, index: Index) -> Option<T> {
        let slot = self.slots.get_mut(index.slot as usize)?;
        if slot.generation != index.generation {
            return None;
        }
        if matches!(slot.entry, Entry::Free(_)) {
            return None;
        }
        // Bump the generation so any other handle to this slot becomes stale,
        // then push the slot onto the free list.
        slot.generation = slot.generation.wrapping_add(1);
        let freed = core::mem::replace(&mut slot.entry, Entry::Free(self.free_head));
        self.free_head = index.slot;
        self.len -= 1;
        match freed {
            Entry::Occupied(value) => Some(value),
            Entry::Free(_) => unreachable!("occupancy was checked above"),
        }
    }

    /// Iterates over `(Index, &T)` for every live value, in slot order.
    pub fn iter(&self) -> impl Iterator<Item = (Index, &T)> {
        self.slots
            .iter()
            .enumerate()
            .filter_map(|(i, slot)| match &slot.entry {
                Entry::Occupied(value) => Some((
                    Index {
                        slot: i as u32,
                        generation: slot.generation,
                    },
                    value,
                )),
                Entry::Free(_) => None,
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_get_roundtrip() {
        let mut arena = Arena::new();
        let a = arena.insert("a");
        let b = arena.insert("b");
        assert_eq!(arena.len(), 2);
        assert_eq!(arena.get(a), Some(&"a"));
        assert_eq!(arena.get(b), Some(&"b"));
        assert_ne!(a, b);
    }

    #[test]
    fn get_mut_mutates_in_place() {
        let mut arena = Arena::new();
        let k = arena.insert(1u32);
        *arena.get_mut(k).unwrap() += 41;
        assert_eq!(arena.get(k), Some(&42));
    }

    #[test]
    fn remove_returns_value_and_frees() {
        let mut arena = Arena::new();
        let k = arena.insert("x");
        assert_eq!(arena.remove(k), Some("x"));
        assert!(arena.is_empty());
        assert_eq!(arena.get(k), None);
    }

    #[test]
    fn double_remove_is_a_noop() {
        let mut arena = Arena::new();
        let k = arena.insert(7);
        assert_eq!(arena.remove(k), Some(7));
        assert_eq!(arena.remove(k), None);
    }

    #[test]
    fn stale_handle_does_not_alias_recycled_slot() {
        // This is the whole point of the generation counter.
        let mut arena = Arena::new();
        let old = arena.insert("first");
        arena.remove(old);
        let new = arena.insert("second");

        // The new occupant reuses the same physical slot...
        assert_eq!(old.slot(), new.slot(), "expected slot reuse for this test");
        // ...but the stale handle must NOT resolve to the new value.
        assert_eq!(arena.get(old), None);
        assert!(!arena.contains(old));
        assert_eq!(arena.get(new), Some(&"second"));
        assert_ne!(old, new);
    }

    #[test]
    fn free_list_reuses_slots_lifo() {
        let mut arena = Arena::new();
        let a = arena.insert(0);
        let b = arena.insert(1);
        arena.insert(2);
        arena.remove(a);
        arena.remove(b);
        // Two frees, two inserts -> no growth beyond the original 3 slots.
        arena.insert(10);
        arena.insert(11);
        assert_eq!(arena.slots.len(), 3);
        assert_eq!(arena.len(), 3);
    }

    #[test]
    fn iter_yields_only_live_entries() {
        let mut arena = Arena::new();
        let a = arena.insert("a");
        let b = arena.insert("b");
        let c = arena.insert("c");
        arena.remove(b);

        let mut live: Vec<_> = arena.iter().map(|(idx, v)| (idx, *v)).collect();
        live.sort_by_key(|(idx, _)| idx.slot());
        assert_eq!(live, alloc::vec![(a, "a"), (c, "c")]);
    }

    #[test]
    fn get_on_out_of_range_index_is_none() {
        let arena: Arena<i32> = Arena::new();
        let bogus = Index {
            slot: 999,
            generation: 0,
        };
        assert_eq!(arena.get(bogus), None);
    }
}
