//! Pure-Rust reactive primitives for composing components *from code*.
//!
//! These carry no Qt dependency: they let you model application state and wire
//! it together in idiomatic Rust, then push it into a [`crate::Model`] to reach
//! QML. Keeping them Qt-free means the state layer of an app stays unit-testable
//! without a running event loop.

/// A change subscriber, invoked with the property's new value.
type Subscriber<T> = Box<dyn FnMut(&T)>;

/// An observable cell. Setting a new value notifies every subscriber.
pub struct Property<T> {
    value: T,
    subscribers: Vec<Subscriber<T>>,
}

impl<T> Property<T> {
    pub fn new(value: T) -> Self {
        Property {
            value,
            subscribers: Vec::new(),
        }
    }

    pub fn get(&self) -> &T {
        &self.value
    }

    /// Replaces the value and notifies subscribers with the new value.
    pub fn set(&mut self, value: T) {
        self.value = value;
        self.notify();
    }

    /// Mutates the value in place, then notifies subscribers.
    pub fn update(&mut self, f: impl FnOnce(&mut T)) {
        f(&mut self.value);
        self.notify();
    }

    /// Registers a subscriber, invoked on every subsequent change.
    pub fn subscribe(&mut self, subscriber: impl FnMut(&T) + 'static) {
        self.subscribers.push(Box::new(subscriber));
    }

    fn notify(&mut self) {
        for sub in &mut self.subscribers {
            sub(&self.value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;

    #[test]
    fn notifies_on_set() {
        let seen = Rc::new(Cell::new(0));
        let mut prop = Property::new(0);
        {
            let seen = seen.clone();
            prop.subscribe(move |v| seen.set(*v));
        }
        prop.set(42);
        assert_eq!(seen.get(), 42);
        assert_eq!(*prop.get(), 42);
    }

    #[test]
    fn update_notifies() {
        let count = Rc::new(Cell::new(0));
        let mut prop = Property::new(vec![1, 2]);
        {
            let count = count.clone();
            prop.subscribe(move |v| count.set(v.len()));
        }
        prop.update(|v| v.push(3));
        assert_eq!(count.get(), 3);
    }
}
