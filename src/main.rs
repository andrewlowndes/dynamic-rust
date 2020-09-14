#![feature(fn_traits)]

use std::cell::RefCell;
use std::ops::Fn;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock, RwLockReadGuard, Weak};

trait CanInvalidate {
    fn invalidate(&self);
}

trait CanLink<'a> {
    fn link(&self, link: Weak<dyn CanInvalidate + 'a>);
    fn strong_link(&self, link: Arc<dyn CanInvalidate + 'a>);
}

trait CanGet<T> {
    fn get(&self) -> RwLockReadGuard<T>;
}

struct Value<'a, T> {
    value: RwLock<T>,
    links: RwLock<Vec<Weak<dyn CanInvalidate + 'a>>>,
    strong_links: RwLock<Vec<Arc<dyn CanInvalidate + 'a>>>,
}

impl<'a, T> Value<'a, T> {
    fn new(value: T) -> Arc<Self> {
        Arc::new(Value {
            value: RwLock::new(value),
            links: RwLock::new(vec![]),
            strong_links: RwLock::new(vec![]),
        })
    }
    fn set(&self, new_value: T) {
        *self.value.write().unwrap() = new_value;
        self.invalidate();
    }
}

impl<'a, T> CanInvalidate for Value<'a, T> {
    fn invalidate(&self) {
        for link in &*self.links.read().unwrap() {
            link.upgrade().unwrap().invalidate();
        }

        for link in &*self.strong_links.read().unwrap() {
            link.invalidate();
        }
    }
}

impl<'a, T> CanLink<'a> for Value<'a, T> {
    fn link(&self, link: Weak<dyn CanInvalidate + 'a>) {
        self.links.write().unwrap().push(link);
    }
    fn strong_link(&self, link: Arc<dyn CanInvalidate + 'a>) {
        self.strong_links.write().unwrap().push(link);
    }
}

impl<'a, T> CanGet<T> for Value<'a, T> {
    fn get(&self) -> RwLockReadGuard<T> {
        self.value.read().unwrap()
    }
}

struct Derived<'a, T: Default, F: Fn() -> T> {
    func: F,
    value: RwLock<T>,
    dirty: AtomicBool,
    links: RwLock<Vec<Weak<dyn CanInvalidate + 'a>>>,
    strong_links: RwLock<Vec<Arc<dyn CanInvalidate + 'a>>>,
}

impl<'a, T: Default, F: Fn() -> T> Derived<'a, T, F> {
    fn new(func: F) -> Arc<Self> {
        let result = Arc::new(Derived {
            func,
            value: RwLock::new(T::default()),
            dirty: AtomicBool::new(true),
            links: RwLock::new(vec![]),
            strong_links: RwLock::new(vec![]),
        });

        result
    }
}

impl<'a, T: Default, F: Fn() -> T> CanInvalidate for Derived<'a, T, F> {
    fn invalidate(&self) {
        self.dirty.store(true, Ordering::Relaxed);

        for link in &*self.links.read().unwrap() {
            link.upgrade().unwrap().invalidate();
        }

        for link in &*self.strong_links.read().unwrap() {
            link.invalidate();
        }
    }
}

impl<'a, T: Default, F: Fn() -> T> CanLink<'a> for Derived<'a, T, F> {
    fn link(&self, link: Weak<dyn CanInvalidate + 'a>) {
        self.links.write().unwrap().push(link);
    }
    fn strong_link(&self, link: Arc<dyn CanInvalidate + 'a>) {
        self.strong_links.write().unwrap().push(link);
    }
}

impl<'a, T: Default, F: Fn() -> T> CanGet<T> for Derived<'a, T, F> {
    fn get(&self) -> RwLockReadGuard<T> {
        if self.dirty.load(Ordering::Relaxed) {
            self.dirty.store(false, Ordering::Relaxed);

            let new_value = (self.func)();
            {
                *self.value.write().unwrap() = new_value;
            }
        }

        self.value.read().unwrap()
    }
}

struct React<'a, F: Fn()> {
    func: F,
    links: RwLock<Vec<Weak<dyn CanInvalidate + 'a>>>,
    strong_links: RwLock<Vec<Arc<dyn CanInvalidate + 'a>>>,
}

impl<'a, F: Fn()> React<'a, F> {
    fn new(func: F) -> Arc<Self> {
        (func)();

        Arc::new(React {
            func,
            links: RwLock::new(vec![]),
            strong_links: RwLock::new(vec![]),
        })
    }
}

impl<'a, F: Fn()> CanInvalidate for React<'a, F> {
    fn invalidate(&self) {
        (self.func)();

        for link in &*self.links.read().unwrap() {
            link.upgrade().unwrap().invalidate();
        }

        for link in &*self.strong_links.read().unwrap() {
            link.invalidate();
        }
    }
}

impl<'a, F: Fn()> CanLink<'a> for React<'a, F> {
    fn link(&self, link: Weak<dyn CanInvalidate + 'a>) {
        self.links.write().unwrap().push(link);
    }
    fn strong_link(&self, link: Arc<dyn CanInvalidate + 'a>) {
        self.strong_links.write().unwrap().push(link);
    }
}

macro_rules! value {
    ($expr:expr) => {
        Value::new($expr)
    };
}

macro_rules! derive {
    ($func:expr, $x:expr, $($y:expr),+) => {
        {
            let clones = (($x.clone() as Arc<dyn CanGet<_>>), $($y.clone() as Arc<dyn CanGet<_>>),+);
            let result = Derived::new(move || Fn::call(&$func, clones.clone()));
            $x.link(Arc::downgrade(&(result.clone() as Arc<dyn CanInvalidate>)));
            $($y.link(Arc::downgrade(&(result.clone() as Arc<dyn CanInvalidate>)));)*
            result
        }
    };
    ($func:expr, $x:expr) => {
        {
            let clone = $x.clone() as Arc<dyn CanGet<_>>;
            let result = Derived::new(move || $func(clone.clone()));
            $x.link(Arc::downgrade(&(result.clone() as Arc<dyn CanInvalidate>)));
            result
        }
    };
    ($func:expr) => {
        Derived::new($func)
    };
}

macro_rules! react {
    ($func:expr, $x:expr, $($y:expr),+) => {
        {
            let clones = (($x.clone() as Arc<dyn CanGet<_>>), $($y.clone() as Arc<dyn CanGet<_>>),+);
            let result = React::new(move || Fn::call(&$func, clones.clone()));
            $x.strong_link(result.clone() as Arc<dyn CanInvalidate>);
            $($y.strong_link(result.clone() as Arc<dyn CanInvalidate>);)*
            result
        }
    };
    ($func:expr, $x:expr) => {
        {
            let clone = $x.clone() as Arc<dyn CanGet<_>>;
            let result = React::new(move || $func(clone.clone()));
            $x.strong_link(result.clone() as Arc<dyn CanInvalidate>);
            result
        }
    };
    ($func:expr) => {
        React::new($func)
    };
}

use std::ops::Add;
fn add<T: Add + Copy>(a: Arc<dyn CanGet<T>>, b: Arc<dyn CanGet<T>>) -> T::Output {
    *a.get() + *b.get()
}

fn main() {
    let a = value!(2);
    let b = value!(4);

    //a derived elememt - lazily evaluated, memorizes it's last return value
    let c = derive!(add, a, b);

    let d = derive!(add, c, c); //FIXME: this will result in duplication if we use the args for calling the method and for registering the callback

    //a reactive elememt - one that is called whenever a dependency is changed with no memorization
    let f = Arc::new(RefCell::new(10));

    let f_clone = f.clone();
    react!(
        |d: Arc<dyn CanGet<_>>| *f_clone.borrow_mut() = *d.get() * 2,
        d
    );

    println!("{} {} {}", c.get(), d.get(), f.borrow());

    a.set(6);

    println!("{} {} {}", c.get(), d.get(), f.borrow());
}
