use core::hash::Hash;
use im::{HashMap, Vector};

pub trait OrDefault<K, V> {
    fn get_or_default(&self, item: &K) -> V;
}

impl<K, V> OrDefault<K, V> for HashMap<K, V>
where
    K: Eq + PartialEq + Hash,
    V: Default + Clone,
{
    fn get_or_default(&self, item: &K) -> V {
        match self.get(item) {
            Some(v) => v.clone(),
            None => V::default(),
        }
    }
}

pub trait PushImmut<T> {
    fn push(&self, item: T) -> Vector<T>;
}
impl<T: Clone> PushImmut<T> for Vector<T> {
    fn push(&self, item: T) -> Vector<T> {
        let mut result = self.clone();
        result.push_back(item);
        result
    }
}

pub trait RemoveImmut<T> {
    fn remove_idx(&self, idx: usize) -> Vector<T>;
}
impl<T: Clone> RemoveImmut<T> for Vector<T> {
    fn remove_idx(&self, idx: usize) -> Vector<T> {
        let mut result = self.clone();
        result.remove(idx);
        result
    }
}
