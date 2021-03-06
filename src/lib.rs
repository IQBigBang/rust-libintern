#![feature(negative_impls)]

use std::{borrow::Borrow, fmt::{Debug, Display}, hash::Hash, marker::PhantomData, ops::Deref, pin::Pin};

/// The interner.
///
/// An interner is a structure which uniquely owns the interned items,
/// and provides shared immutable references to those items.
pub struct Interner<'a, T: 'a + Eq> {
    /// A list of holders of the items
    holders: Vec<InternedItemHolder<T>>,
    _ph: PhantomData<&'a T>
}

impl<'a, T> !Sync for Interner<'a, T> {}

/// The capacity of the first InternedItemHolder
const BEGIN_INTERNER_CAPACITY: usize = 32;
/// By how much every next interner's capacity changes
const INTERNER_CAPACITY_DELTA: f32 = 1.5;

impl<'a, T: 'a + Eq> Interner<'a, T> {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self { 
            holders: vec![
                InternedItemHolder::new(BEGIN_INTERNER_CAPACITY)],
            _ph: PhantomData 
        }
    }

    /// Intern an item.
    ///
    /// This consumes the item by adding it to the intern-list and returns a reference to it.
    /// It also extends the lifetime of the item to match the lifetime of this interner.
    ///
    /// This item is dropped if an item equal to this one is already interned,
    /// in which case a reference to the already interned item is returned instead.
    pub fn intern(&mut self, item: T) -> Intern<'a, T> {
        // Look whether an item equal to this one already exists
        let mut result = None;
        for holder in &self.holders {
            for h_item in &holder.items {
                if &item == h_item {
                    result = Some(h_item);
                    break
                }
            }
        }
        // If the new item is unique, add it to the holder
        if result.is_none() {
            self.hold_new_item(item);
            result = Some(
                // See documentation for [`hold_new_item`]
                self.holders.last().unwrap().items.last().unwrap()
            )
        }
        let reference = result.unwrap();
        unsafe { self.transmute_held_item(reference) }
    }

    pub fn contains(&self, item: &T) -> bool {
        for holder in &self.holders {
            for h_item in &holder.items {
                if item == h_item {
                    return true
                }
            }
        }
        false
    }

    /// Hold a new item.
    /// If the currently last holder is full, create a new holder.
    ///
    /// The new item is guaranteed to be placed as the last item of the last holder
    fn hold_new_item(&mut self, item: T) {
        match self.holders.last_mut().unwrap().try_push(item) {
            Ok(()) => (),
            Err(item) => {
                // The holder is full, add a new one
                let last_holder_capacity = self.holders.last().unwrap().items.capacity();
                let mut new_holder = InternedItemHolder::new(
                    ((last_holder_capacity as f32) * INTERNER_CAPACITY_DELTA) as usize
                );
                // Add to the holder
                new_holder.items.push(item);
                // Add the holder to the list of holders
                self.holders.push(new_holder);
            }
        }
    }

    /// Transmute a reference to an item held by this interner
    /// into the Intern<T> type.
    #[inline]
    unsafe fn transmute_held_item(&self, item: &T) -> Intern<'a, T> {
        // SAFETY: Via the lifetime <'a>, we guarantee the interner is alive
        // as long as the references are alive. Furthermore, the data is NEVER
        // mutated AND only immutable references to the data exist.
        // Therefore we uphold all guarantees and can assume safety when transmuting
        let reference: &'a T = std::mem::transmute(item);
        // SAFETY: I believe for the reasons stated above, this is also safe
        let pinned_reference: Pin<&'a T> = Pin::new_unchecked(reference);
        Intern(pinned_reference)
    }

    pub fn iter<'this>(&'this self) -> Iter<'this, 'a, T> {
        Iter { interner: self, holder_id: 0, inside_holder_id: 0 }
    }
}

/// A wrapper around a vector, which guarantees that
/// the vector will never grow, thus the addresses (pointers)
/// of (to) its items will never change
struct InternedItemHolder<T> {
    items: Vec<T>
}

impl<T> InternedItemHolder<T> {
    fn new(capacity: usize) -> Self {
        Self { items: Vec::with_capacity(capacity) }
    }

    /// Try to add an item to the holder.
    ///
    /// If there's enough space for the item, succeed and return Ok(())
    /// If there's not enough space in the holder,
    ///  it returns Err(the_item), to prevent dropping the value
    fn try_push(&mut self, item: T) -> Result<(), T> {
        if self.items.len() == self.items.capacity() {
            Err(item)
        } else {
            self.items.push(item);
            Ok(())
        }
    }
}

/// A reference to an interned item.
///
/// The main advantage of this type over just
/// an immutable reference is that its pointer is guaranteed to be unique
/// within a single [`Interner`]. Therefore comparisons
/// are very cheap.
///
/// # Note about hashing
///
/// This wrapper implements [`PartialEq`] by comparing its inner pointer.
/// In order to keep consistency, the [`Hash`] trait is also implemented
/// by hashing the pointer, NOT the inner value. Therefore hashes
/// of the `Intern<T>` type are different than hashes of the `T`.
pub struct Intern<'a, T: 'a>(Pin<&'a T>);

impl<'a, T> Clone for Intern<'a, T> {
    fn clone(&self) -> Self {
        Intern(self.0)
    }
}

// We must hand implement Copy, because a T: Copy bound is added when using derive
impl<'a, T> Copy for Intern<'a, T> {}

// Get reference to the inner item
impl<'a, T> AsRef<T> for Intern<'a, T> {
    fn as_ref(&self) -> &T {
        self.0.get_ref()
    }
}

impl<'a, T> Borrow<T> for Intern<'a, T> {
    fn borrow(&self) -> &T {
        self.0.get_ref()
    }
}

impl<'a, T> Deref for Intern<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.as_ref()
    }
}

// Implement Debug if the item implements Debug
impl<'a, T: Debug> Debug for Intern<'a, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.as_ref().fmt(f)
    }
}

// Implement Display if the item implements Display
impl<'a, T: Display> Display for Intern<'a, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.as_ref().fmt(f)
    }
}

// Implement PartialEq
// 
// Because we can guarantee that if the item is the same,
// the item's place in memory, therefore the pointer is the same,
// we can just compare values of the pointers, not the items themselves 
impl<'a, T> PartialEq for Intern<'a, T> {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.as_ref() as *const _, other.as_ref() as *const _)
    }
}

impl<'a, T> Eq for Intern<'a, T> {}

// Implement Hash
// 
/// To keep consistency with [`PartialEq`], we hash the pointer, not the value
impl<'a, T> Hash for Intern<'a, T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::ptr::hash(self.0.get_ref(), state)
    }
}

// Implement PartialOrd and Ord if the item implements it
impl<'a, T: PartialOrd> PartialOrd for Intern<'a, T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.0.partial_cmp(&other.0)
    }
}

impl<'a, T: Ord> Ord for Intern<'a, T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.cmp(&other.0)
    }
}

impl<'a, T> Intern<'a, T> {
    /// Create a new [`Intern`] type from a raw pointer.
    ///
    /// # Safety
    /// The caller must guarantee that the pointed value is in fact
    /// owned by an [`Interner`] which is still in scope (i.e. not dropped)
    /// and that the pointer will not be mutated and/or moved.
    pub unsafe fn from_raw(ptr: *const T) -> Self {
        Intern(Pin::new_unchecked(ptr.as_ref().unwrap()))
    }
}

pub struct Iter<'a, 'intern, T: std::cmp::Eq> {
    interner: &'a Interner<'intern, T>,
    holder_id: usize,
    inside_holder_id: usize
}

impl<'a, 'intern, T: std::cmp::Eq> Iterator for Iter<'a, 'intern, T> {
    type Item = Intern<'intern, T>;

    fn next(&mut self) -> Option<Self::Item> {
        let holder = self.interner.holders.get(self.holder_id)?;
        let item = &holder.items[self.inside_holder_id];
        if self.inside_holder_id == holder.items.len() - 1 {
            //if this is the last item inside this holder
            self.holder_id += 1;
            self.inside_holder_id = 0;
        } else {
            self.inside_holder_id += 1;
        }
        Some(unsafe { self.interner.transmute_held_item(item) })
    }
}


#[cfg(test)]
mod tests {
    use super::{InternedItemHolder, Interner};

    #[test]
    fn interned_item_holder_test() {
        let mut holder = InternedItemHolder::new(4); // size four

        // Add an item
        assert!(holder.try_push('a').is_ok());
        assert!(holder.items.len() == 1);
        assert!(holder.items.capacity() == 4);
        // Save the address of the item
        let first_item_address = holder.items.get(0).unwrap() as *const _ as usize;

        // Add another item
        assert!(holder.try_push('b').is_ok());
        assert!(holder.items.len() == 2);
        assert!(holder.items.capacity() == 4);
        // Make sure the address of the first one didn't change
        assert_eq!(
            holder.items.get(0).unwrap() as *const _ as usize,
            first_item_address
        );
        let second_item_address = holder.items.get(1).unwrap() as *const _ as usize;

        // Add two more items
        assert!(holder.try_push('c').is_ok());
        assert!(holder.try_push('d').is_ok());
        assert!(holder.items.len() == 4);
        assert!(holder.items.capacity() == 4);
        // Make sure the addresses didn't change
        assert_eq!(
            holder.items.get(0).unwrap() as *const _ as usize,
            first_item_address
        );
        assert_eq!(
            holder.items.get(1).unwrap() as *const _ as usize,
            second_item_address
        );

        // Try to add more items
        assert_eq!(holder.try_push('e'), Err('e'));
        assert_eq!(holder.try_push('f'), Err('f'));
        assert!(holder.items.len() == 4);
        assert!(holder.items.capacity() == 4);
        // Make sure the addresses didn't change
        assert_eq!(
            holder.items.get(0).unwrap() as *const _ as usize,
            first_item_address
        );
        assert_eq!(
            holder.items.get(1).unwrap() as *const _ as usize,
            second_item_address
        );

        // Try to dereference the addresses, just to be sure
        assert_eq!(
            unsafe { *(first_item_address as *const char) },
            'a');
        assert_eq!(
            unsafe { *(second_item_address as *const char) },
            'b');
    }

    #[test]
    fn interner_test() {
        let mut int = Interner::new();
        // Intern some things
        let ref_a1 = int.intern('a');
        let ref_b = int.intern('b');
        let ref_a2 = int.intern('a');
        // After this, only TWO items should be interned 'a' and 'b'. The second 'a' should have been discarded
        assert_eq!(int.holders.len(), 1);
        assert_eq!(int.holders[0].items.len(), 2);
        // Now check that the addresses of ref_a1 and ref_a2 are equal
        assert!(std::ptr::eq(ref_a1.as_ref(), ref_a2.as_ref()));
        assert!(!std::ptr::eq(ref_a1.as_ref(), ref_b.as_ref()));

        let ref_b2 = int.intern('b');
        let _ref_c = int.intern('c');
        assert_eq!(ref_b, ref_b2);
    }

    #[test]
    fn intern_impl_test() {
        let mut int = Interner::new();
        let a1 = int.intern('a');
        let a2 = int.intern('a');
        let x = int.intern('x');

        // AsRef
        assert_eq!(a1.as_ref(), &'a');
        // Borrow
        assert_eq!(<_ as std::borrow::Borrow<char>>::borrow(&a1), &'a');
        // Deref
        assert_eq!(*a1, 'a');
        // TODO: Debug and Display test
        // PartialEq
        assert_eq!(a1, a2);
        assert_ne!(a1, x);
        // TODO: Hash test
    }

    #[test]
    fn interner_iter_test() {
        let mut int = Interner::new();
        for i in 0..100 {
            int.intern(i);
        }

        let collected: Vec<i32> = int.iter().map(|i| *i).collect();

        assert_eq!(
            collected,
            (0..100).collect::<Vec<i32>>()
        );
    }
}
