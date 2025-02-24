use {
    grug_types::{increment_last_byte, Batch, Order, Record, Storage},
    std::{
        sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard},
        vec,
    },
};

pub struct SharedStore<S> {
    store: Arc<RwLock<S>>,
}

impl<S> SharedStore<S> {
    pub fn new(store: S) -> Self {
        Self {
            store: Arc::new(RwLock::new(store)),
        }
    }

    pub fn share(&self) -> Self {
        Self {
            store: Arc::clone(&self.store),
        }
    }

    pub fn read_access(&self) -> RwLockReadGuard<S> {
        self.store.read().unwrap_or_else(|err| {
            panic!("poisoned lock: {err:?}")
        })
    }

    pub fn write_access(&self) -> RwLockWriteGuard<S> {
        self.store.write().unwrap_or_else(|err| {
            panic!("poisoned lock: {err:?}")
        })
    }

    /// Disassemble the shared store and return the underlying store.
    /// Fails if there are currently more than one strong reference to it.
    pub fn disassemble(self) -> S {
        let lock = Arc::try_unwrap(self.store).unwrap_or_else(|_| {
            panic!("")
        });

        lock.into_inner().unwrap_or_else(|err| {
            panic!("poisoned lock: {err:?}")
        })
    }
}

impl<S> Clone for SharedStore<S> {
    fn clone(&self) -> Self {
        self.share()
    }
}

impl<S: Storage> Storage for SharedStore<S> {
    fn read(&self, key: &[u8]) -> Option<Vec<u8>> {
        self.read_access().read(key)
    }

    // This is very tricky! Took me days to figure out how to do `scan` on a
    // shared store.
    //
    // A naive implementation of the `scan` may be something like this:
    //
    // ```rust
    // let store = self.store.borrow();
    // store.scan(min, max, order)
    // ```
    //
    // However, this doesn't work! Compiler would complain:
    //
    // > cannot return value referencing local variable `store`
    // > returns a value referencing data owned by the current function
    //
    // Basically, `store` is dropped at the end of the function. The iterator
    // created by `store.scan()` holds an immutable reference to `store`, so it
    // cannot be returned.
    //
    // For this reason, we need to collect the records into a Vec and have it
    // owned by the iterator. However we can't collect the entire [min, max)
    // range either, because it can potentially very big.
    //
    // The solution we have for now is to collect 30 records per batch. If the
    // batch reaches the end, we fetch the next batch. 30 is the default page
    // limit we use in contracts, so it's a reasonable value.
    fn scan<'a>(
        &'a self,
        min: Option<&[u8]>,
        max: Option<&[u8]>,
        order: Order,
    ) -> Box<dyn Iterator<Item = Record> + 'a> {
        Box::new(SharedIter::new(self.read_access(), min, max, order))
    }

    fn write(&mut self, key: &[u8], value: &[u8]) {
        self.write_access().write(key, value)
    }

    fn remove(&mut self, key: &[u8]) {
        self.write_access().remove(key)
    }

    fn flush(&mut self, batch: Batch) {
        self.write_access().flush(batch)
    }
}

struct SharedIter<'a, S> {
    store: RwLockReadGuard<'a, S>,
    batch: vec::IntoIter<Record>,
    min:   Option<Vec<u8>>,
    max:   Option<Vec<u8>>,
    order: Order,
}

impl<'a, S> SharedIter<'a, S> {
    const BATCH_SIZE: usize = 30;

    pub fn new(
        store: RwLockReadGuard<'a, S>,
        min:   Option<&[u8]>,
        max:   Option<&[u8]>,
        order: Order,
    ) -> Self {
        Self {
            store,
            batch: Vec::new().into_iter(),
            min:   min.map(|slice| slice.to_vec()),
            max:   max.map(|slice| slice.to_vec()),
            order,
        }
    }
}

impl<'a, S: Storage> SharedIter<'a, S> {
    fn collect_next_batch(&mut self) {
        let batch = self
            .store
            .scan(self.min.as_deref(), self.max.as_deref(), self.order)
            .take(Self::BATCH_SIZE)
            .collect::<Vec<_>>();

        // now we need to update the bounds
        if let Some((key, _)) = batch.iter().last() {
            match self.order {
                Order::Ascending => self.min = Some(increment_last_byte(key.clone())),
                Order::Descending => self.max = Some(key.clone()),
            }
        }

        self.batch = batch.into_iter();
    }
}

impl<'a, S: Storage> Iterator for SharedIter<'a, S> {
    type Item = Record;

    fn next(&mut self) -> Option<Self::Item> {
        // get the next record in the batch. if it exists (i.e. the batch hasn't
        // reached end yet) then simply return this record
        if let Some(record) = self.batch.next() {
            return Some(record);
        }

        // we're here means the batch has reached end. collect another batch
        // from the store. return the first record in the new batch (which may
        // be None, in which case the entire iteration has reached end)
        self.collect_next_batch();
        self.batch.next()
    }
}

// ----------------------------------- tests -----------------------------------

#[cfg(test)]
mod tests {
    use {super::*, grug_types::MockStorage};

    fn mock_records(min: u32, max: u32, order: Order) -> Vec<Record> {
        let mut records = vec![];
        for i in min..max {
            records.push((i.to_be_bytes().to_vec(), i.to_be_bytes().to_vec()));
        }
        if order == Order::Descending {
            records.reverse();
        }
        records
    }

    #[test]
    fn iterator_works() {
        let mut store = SharedStore::new(MockStorage::new());
        for (k, v) in mock_records(1, 100, Order::Ascending) {
            store.write(&k, &v);
        }

        let records = store
            .scan(Some(&12u32.to_be_bytes()), Some(&89u32.to_be_bytes()), Order::Ascending)
            .collect::<Vec<_>>();
        assert_eq!(records, mock_records(12, 89, Order::Ascending));

        let records = store
            .scan(None, None, Order::Descending)
            .collect::<Vec<_>>();
        assert_eq!(records, mock_records(1, 100, Order::Descending));
    }
}
