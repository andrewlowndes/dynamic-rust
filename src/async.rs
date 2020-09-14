#![feature(fn_traits, async_closure)]

use async_std::sync::{RwLock,RwLockReadGuard,Arc,Weak};
use std::sync::atomic::{AtomicBool,Ordering};
use std::cell::{RefCell};
use std::ops::{Fn};
use async_trait::async_trait;
use std::future::{Future};
use futures::executor::{block_on};

#[async_trait]
trait CanInvalidate {
    async fn invalidate(&self);
}

#[async_trait]
trait CanLink<'a> {
    async fn link(&'a self, link: Weak<dyn CanInvalidate + Send + Sync + 'a>);
    async fn strong_link(&'a self, link: Arc<dyn CanInvalidate + Send + Sync + 'a>);
}

#[async_trait]
trait CanGet<'a, T: Send + Sync + 'a> {
    async fn get(&'a self) -> RwLockReadGuard<T>;
}

struct Value<'a, T: Send + Sync> {
    value: RwLock<T>,
    links: RwLock<Vec<Weak<dyn CanInvalidate + Send + Sync + 'a>>>,
    strong_links: RwLock<Vec<Arc<dyn CanInvalidate + Send + Sync + 'a>>>,
}

impl<'a, T: Send + Sync> Value<'a, T> {
    fn new(value: T) -> Arc<Self> {
        Arc::new(Value { 
            value: RwLock::new(value), 
            links: RwLock::new(vec!()),
            strong_links: RwLock::new(vec!()),
        })
    }
    async fn set(&self, new_value: T) {
        *self.value.write().await = new_value;
        self.invalidate();
    }
}

#[async_trait]
impl<'a, T: Send + Sync> CanInvalidate for Value<'a, T> {
    async fn invalidate(&self) {
        for link in &*self.links.read().await {
            link.upgrade().unwrap().invalidate();
        }

        for link in &*self.strong_links.read().await {
            link.invalidate();
        }
    }
}

#[async_trait]
impl<'a, T: Send + Sync> CanLink<'a> for Value<'a, T> {
    async fn link(&'a self, link: Weak<dyn CanInvalidate + Send + Sync + 'a>) {
        self.links.write().await.push(link);
    }
    async fn strong_link(&'a self, link: Arc<dyn CanInvalidate + Send + Sync + 'a>) {
        self.strong_links.write().await.push(link);
    }
}

#[async_trait]
impl<'a, T: Send + Sync + 'a> CanGet<'a, T> for Value<'a, T> {
    async fn get(&'a self) -> RwLockReadGuard<'a, T> {
        self.value.read().await
    }
}

struct Derived<'a, T: Send + Sync + 'a, O: Future<Output = T>, F: Fn() -> O + Send + Sync> {
    func: F,
    value: RwLock<T>,
    dirty: AtomicBool,
    links: RwLock<Vec<Weak<dyn CanInvalidate + Send + Sync + 'a>>>,
    strong_links: RwLock<Vec<Arc<dyn CanInvalidate + Send + Sync + 'a>>>,
}

impl<'a, T: Send + Sync + 'a, O: Future<Output = T>, F: Fn() -> O + Send + Sync> Derived<'a, T, O, F> {
    fn new(func: F, default_value: T) -> Arc<Self> {
        let result = Arc::new(Derived { 
            func, 
            value: RwLock::new(default_value), 
            dirty: AtomicBool::new(true), 
            links: RwLock::new(vec!()),
            strong_links: RwLock::new(vec!()),
        });

        result
    }
}

#[async_trait]
impl<'a, T: Send + Sync + 'a, O: Future<Output = T>, F: Fn() -> O + Send + Sync> CanInvalidate for Derived<'a, T, O, F> {
    async fn invalidate(&self) {
        self.dirty.store(true, Ordering::Relaxed);

        for link in &*self.links.read().await {
            link.upgrade().unwrap().invalidate();
        }

        for link in &*self.strong_links.read().await {
            link.invalidate();
        }
    }
}

#[async_trait]
impl<'a, T: Send + Sync + 'a, O: Future<Output = T>, F: Fn() -> O + Send + Sync> CanLink<'a> for Derived<'a, T, O, F> {
    async fn link(&'a self, link: Weak<dyn CanInvalidate + Send + Sync + 'a>) {
        self.links.write().await.push(link);
    }
    async fn strong_link(&'a self, link: Arc<dyn CanInvalidate + Send + Sync + 'a>) {
        self.strong_links.write().await.push(link);
    }
}

#[async_trait]
impl<'a, T: Send + Sync + 'a, O: Future<Output = T> + Send, F: Fn() -> O + Send + Sync> CanGet<'a, T> for Derived<'a, T, O, F> {
    async fn get(&'a self) -> RwLockReadGuard<'a, T> {
        if self.dirty.load(Ordering::Relaxed) {
            self.dirty.store(false, Ordering::Relaxed);

            let new_value = (self.func)().await;
            {
                *self.value.write().await = new_value;
            }
        }
        
        self.value.read().await
    }
}

struct React<'a, T: Future + Sync + Send, F: Fn() -> T + Sync + Send> {
    func: F,
    links: RwLock<Vec<Weak<dyn CanInvalidate + Send + Sync + 'a>>>,
    strong_links: RwLock<Vec<Arc<dyn CanInvalidate + Send + Sync + 'a>>>,
}

impl<'a, T: Future + Sync + Send, F: Fn() -> T + Sync + Send> React<'a, T, F> {
    fn new(func: F) -> Arc<Self> {
        (func)();

        let result = Arc::new(React { 
            func, 
            links: RwLock::new(vec!()),
            strong_links: RwLock::new(vec!()),
        });

        result
    }
}

#[async_trait]
impl<'a, T: Future + Sync + Send, F: Fn() -> T + Sync + Send> CanInvalidate for React<'a, T, F> {
    async fn invalidate(&self) {
        (self.func)().await;

        for link in &*self.links.read().await {
            link.upgrade().unwrap().invalidate();
        }

        for link in &*self.strong_links.read().await {
            link.invalidate();
        }
    }
}

#[async_trait]
impl<'a, T: Future + Sync + Send, F: Fn() -> T + Sync + Send> CanLink<'a> for React<'a, T, F> {
    async fn link(&'a self, link: Weak<dyn CanInvalidate + Send + Sync + 'a>) {
        self.links.write().await.push(link);
    }
    async fn strong_link(&'a self, link: Arc<dyn CanInvalidate + Send + Sync + 'a>) {
        self.strong_links.write().await.push(link);
    }
}

/*
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
            $x.link(Arc::downgrade(&(result.clone() as Arc<dyn CanInvalidate + Send + Sync>)));
            $($y.link(Arc::downgrade(&(result.clone() as Arc<dyn CanInvalidate + Send + Sync>)));)*
            result
        }
    };
    ($func:expr, $x:expr) => {
        {
            let clone = $x.clone() as Arc<dyn CanGet<_>>;
            let result = Derived::new(move || $func(clone.clone()));
            $x.link(Arc::downgrade(&(result.clone() as Arc<dyn CanInvalidate + Send + Sync>)));
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
            $x.strong_link(result.clone() as Arc<dyn CanInvalidate + Send + Sync>);
            $($y.strong_link(result.clone() as Arc<dyn CanInvalidate + Send + Sync>);)*
            result
        }
    };
    ($func:expr, $x:expr) => {
        {
            let clone = $x.clone() as Arc<dyn CanGet<_>>;
            let result = React::new(move || $func(clone.clone()));
            $x.strong_link(result.clone() as Arc<dyn CanInvalidate + Send + Sync>);
            result
        }
    };
    ($func:expr) => {
        React::new($func)
    };
}
*/

async fn main_async() {
    let a = Value::new(2);
    let b = Value::new(4);

    let c = {
        let a_clone = a.clone() as Arc<dyn CanGet<'_, _> + Send + Sync>;
        let b_clone = b.clone() as Arc<dyn CanGet<'_, _> + Send + Sync>;

        let result = Derived::new(async move || {
            let a_val = a_clone.get().await;
            let b_val = b_clone.get().await;

            *a_val + *b_val
        }, 0);

        result
    };

    /*
    let a = value!(2);
    let b = value!(4);
    
    //a derived elememt - lazily evaluated, memorizes it's last return value
    let c = derive!(add, a, b);

    let d = derive!(add, c, c); //TODO: this will result in duplication if we use the args for calling the method and for registering the callback

    //a reactive elememt - one that is called whenever a dependency is changed with no memorization
    let f = Arc::new(RefCell::new(10));
    let f_clone = f.clone();

    react!(async move |d: Arc<dyn CanGet<i32>>| *f_clone.borrow_mut() = *d.get().await * 2, d);


    println!("{} {} {}", c.get().await, d.get(), f.borrow());

    //test the propagation of change of the connected components
    a.set(6);

    println!("{} {} {}", c.get(), d.get(), f.borrow());*/
}

fn main() {
    block_on(main_async());
}
