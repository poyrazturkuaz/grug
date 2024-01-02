use {
    crate::{Order, Record, Batch, Committable, Op, Storage},
    std::{cmp::Ordering, iter, iter::Peekable, ops::Bound},
};

/// Adapted from cw-multi-test:
/// https://github.com/CosmWasm/cw-multi-test/blob/v0.19.0/src/transactions.rs#L170-L253
pub struct Cached<S> {
    base:    S,
    pending: Batch,
}

impl<S> Cached<S> {
    /// Create a new cached store with an empty write batch.
    pub fn new(base: S) -> Self {
        Self {
            base,
            pending: Batch::new(),
        }
    }

    /// Comsume self, discard the uncommitted batch, return the underlying store.
    pub fn recycle(self) -> S {
        self.base
    }
}

impl<S> Committable for Cached<S> {
    fn apply(&mut self, batch: Batch) -> anyhow::Result<()> {
        // this merges the two batches, with the incoming batch taking precedence.
        self.pending.extend(batch);
        Ok(())
    }
}

impl<S: Committable> Cached<S> {
    /// Consume the cached store, write all ops to the underlying store, return
    /// the underlying store.
    pub fn commit(mut self) -> anyhow::Result<S> {
        self.base.apply(self.pending)?;
        Ok(self.base)
    }
}

impl<S: Storage> Storage for Cached<S> {
    fn read(&self, key: &[u8]) -> anyhow::Result<Option<Vec<u8>>> {
        match self.pending.get(key) {
            Some(Op::Put(value)) => Ok(Some(value.clone())),
            Some(Op::Delete) => Ok(None),
            None => self.base.read(key),
        }
    }

    fn scan<'a>(
        &'a self,
        min:   Option<&[u8]>,
        max:   Option<&[u8]>,
        order: Order,
    ) -> anyhow::Result<Box<dyn Iterator<Item = Record> + 'a>> {
        if let (Some(min), Some(max)) = (min, max) {
            if min > max {
                return Ok(Box::new(iter::empty()));
            }
        }

        let base = self.base.scan(min, max, order)?;

        let min = min.map_or(Bound::Unbounded, |bytes| Bound::Included(bytes.to_vec()));
        let max = max.map_or(Bound::Unbounded, |bytes| Bound::Excluded(bytes.to_vec()));
        let pending_raw = self.pending.range((min, max));
        let pending: Box<dyn Iterator<Item = _>> = match order {
            Order::Ascending => Box::new(pending_raw),
            Order::Descending => Box::new(pending_raw.rev()),
        };

        Ok(Box::new(MergedIter::new(base, pending, order)))
    }

    fn write(&mut self, key: &[u8], value: &[u8]) -> anyhow::Result<()> {
        self.pending.insert(key.to_vec(), Op::Put(value.to_vec()));
        Ok(())
    }

    fn remove(&mut self, key: &[u8]) -> anyhow::Result<()> {
        self.pending.insert(key.to_vec(), Op::Delete);
        Ok(())
    }
}

struct MergedIter<'a, B, P>
where
    B: Iterator<Item = Record>,
    P: Iterator<Item = (&'a Vec<u8>, &'a Op)>
{
    base:    Peekable<B>,
    pending: Peekable<P>,
    order:   Order,
}

impl<'a, B, P> MergedIter<'a, B, P>
where
    B: Iterator<Item = Record>,
    P: Iterator<Item = (&'a Vec<u8>, &'a Op)>
{
    pub fn new(base: B, pending: P, order: Order) -> Self {
        Self {
            base:    base.peekable(),
            pending: pending.peekable(),
            order,
        }
    }

    fn take_pending(&mut self) -> Option<Record> {
        let Some((key, op)) = self.pending.next() else {
            return None;
        };

        match op {
            Op::Put(value) => Some((key.clone(), value.clone())),
            Op::Delete => self.next(),
        }
    }
}

impl<'a, B, P> Iterator for MergedIter<'a, B, P>
where
    B: Iterator<Item = Record>,
    P: Iterator<Item = (&'a Vec<u8>, &'a Op)>
{
    type Item = Record;

    fn next(&mut self) -> Option<Self::Item> {
        match (self.base.peek(), self.pending.peek()) {
            (Some((base_key, _)), Some((pending_key, _))) => {
                let ordering_raw = base_key.cmp(pending_key);
                let ordering = match self.order {
                    Order::Ascending => ordering_raw,
                    Order::Descending => ordering_raw.reverse(),
                };

                match ordering {
                    Ordering::Less => self.base.next(),
                    Ordering::Equal => {
                        self.base.next();
                        self.take_pending()
                    },
                    Ordering::Greater => self.take_pending(),
                }
            }
            (None, Some(_)) => self.take_pending(),
            (Some(_), None) => self.base.next(),
            (None, None) => None,
        }
    }
}

// ----------------------------------- tests -----------------------------------

#[cfg(test)]
mod tests {
    use {super::*, crate::MockStorage};

    // illustration of this test case:
    //
    // base    : 1 2 _ 4 5 6 7 _
    // pending :   D P _ _ P D 8  (P = put, D = delete)
    // merged  : 1 _ 3 4 5 6 _ 8
    fn make_test_case() -> anyhow::Result<(Cached<MockStorage>, Vec<Record>)> {
        let mut base = MockStorage::new();
        base.write(&[1], &[1])?;
        base.write(&[2], &[2])?;
        base.write(&[4], &[4])?;
        base.write(&[5], &[5])?;
        base.write(&[6], &[6])?;
        base.write(&[7], &[7])?;

        let mut cached = Cached::new(base);
        cached.remove(&[2])?;
        cached.write(&[3], &[3])?;
        cached.write(&[6], &[255])?;
        cached.remove(&[7])?;
        cached.write(&[8], &[8])?;

        let merged = vec![
            (vec![1], vec![1]),
            (vec![3], vec![3]),
            (vec![4], vec![4]),
            (vec![5], vec![5]),
            (vec![6], vec![255]),
            (vec![8], vec![8]),
        ];

        Ok((cached, merged))
    }

    #[test]
    fn iterator_works() -> anyhow::Result<()> {
        let (cached, mut merged) = make_test_case()?;
        assert_eq!(cached.to_vec(Order::Ascending)?, merged);

        merged.reverse();
        assert_eq!(cached.to_vec(Order::Descending)?, merged);

        Ok(())
    }

    #[test]
    fn commit_works() -> anyhow::Result<()> {
        let (cached, merged) = make_test_case()?;

        let base = cached.commit()?;
        assert_eq!(base.to_vec(Order::Ascending)?, merged);

        Ok(())
    }

    // TODO: add fuzz test
}