use {
    crate::{from_json, to_json, Storage},
    anyhow::Context,
    data_encoding::BASE64,
    serde::{de::DeserializeOwned, ser::Serialize},
    std::{any::type_name, marker::PhantomData},
};

pub struct Item<T> {
    key:        &'static [u8],
    _data_type: PhantomData<T>,
}

impl<T> Item<T> {
    pub const fn new(key: &'static str) -> Self {
        Self {
            key:        key.as_bytes(),
            _data_type: PhantomData,
        }
    }
}

impl<T> Item<T>
where
    T: Serialize + DeserializeOwned,
{
    pub fn exists(&self, store: &dyn Storage) -> bool {
        store.read(self.key).is_some()
    }

    pub fn may_load(&self, store: &dyn Storage) -> anyhow::Result<Option<T>> {
        store
            .read(self.key)
            .map(from_json) // TODO: add more informative error msg
            .transpose()
            .map_err(Into::into)
    }

    pub fn load(&self, store: &dyn Storage) -> anyhow::Result<T> {
        from_json(&store
            .read(self.key)
            .with_context(|| format!(
                "[Item]: data not found! type: {}, key: {}",
                type_name::<T>(),
                BASE64.encode(self.key),
            ))?)
            .map_err(Into::into)
    }

    pub fn update<A>(&self, store: &mut dyn Storage, action: A) -> anyhow::Result<Option<T>>
    where
        A: FnOnce(Option<T>) -> anyhow::Result<Option<T>>,
    {
        let maybe_data = action(self.may_load(store)?)?;

        if let Some(data) = &maybe_data {
            self.save(store, data)?;
        } else {
            self.remove(store);
        }

        Ok(maybe_data)
    }

    pub fn save(&self, store: &mut dyn Storage, data: &T) -> anyhow::Result<()> {
        let bytes = to_json(data)?;
        store.write(self.key, &bytes);
        Ok(())
    }

    pub fn remove(&self, store: &mut dyn Storage) {
        store.remove(self.key)
    }
}