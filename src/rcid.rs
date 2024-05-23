use std::cmp::Ordering;
use std::hash::{Hash, Hasher};
use std::ops::Deref;
use std::rc::Rc;

#[derive(Debug)]
#[repr(transparent)]
pub struct RcId<T>(Rc<T>);
impl<T> RcId<T> {
    pub fn new(value: T) -> Self {
        RcId(Rc::new(value))
    }
}

impl<T> Clone for RcId<T> {
    fn clone(&self) -> Self {
        RcId(self.0.clone())
    }
}

impl<T> Hash for RcId<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        Rc::as_ptr(&self.0).hash(state)
    }
}
impl<T> PartialEq for RcId<T> {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}
impl<T> Eq for RcId<T> {}
impl<T> PartialOrd for RcId<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl<T> Ord for RcId<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        Rc::as_ptr(&self.0).cmp(&Rc::as_ptr(&other.0))
    }
}

impl <T> Deref for RcId<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}