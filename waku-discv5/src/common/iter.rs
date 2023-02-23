use std::iter::{Fuse, FusedIterator};

pub(crate) trait DrainableFusedIterator: FusedIterator {
    fn drain(&mut self) {
        while let Some(_) = self.next() {}
    }
}

impl<I> DrainableFusedIterator for Fuse<I> where I: Iterator {}
