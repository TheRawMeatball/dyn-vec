use std::fmt::Debug;

use dyn_vec::{dyn_vec_usable, DynVec};

fn main() {
    let mut dynvec = DynVec::<dyn Example>::new();
    dynvec.push(i32::MAX);
    dynvec.push(u64::MAX);
    dynvec.push(u128::from_ne_bytes(*b"deadbeef__foobar"));
    dynvec.push(vec![13, 26]);
    for r in dynvec.iter() {
        r.uses_ref();
    }
    dynvec.drain().for_each(|_| {});
    dynvec.push("lol");
    for mut r in dynvec.drain() {
        r.as_mut_dyn_ref();
        r.takes_ownership();
    }
}

#[dyn_vec_usable]
pub trait Example {
    fn uses_ref(&self);
    fn takes_ownership(self);
}

impl<T: Debug> Example for T {
    fn uses_ref(&self) {
        dbg!(self);
    }

    fn takes_ownership(self) {
        dbg!(self);
    }
}
