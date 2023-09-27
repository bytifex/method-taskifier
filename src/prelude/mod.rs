mod result_option_inspect;

use std::{
    any::{Any, TypeId},
    sync::Arc,
};

use parking_lot::Mutex;

pub use result_option_inspect::*;

pub type ArcMutex<T> = Arc<Mutex<T>>;

pub trait DowncastArc {
    fn downcast_arc<CastType: 'static>(&self) -> Option<Arc<CastType>>;
}

impl<T> DowncastArc for Arc<T>
where
    T: ?Sized + Any + 'static,
{
    fn downcast_arc<CastType: 'static>(&self) -> Option<Arc<CastType>> {
        let arc_clone = self.clone();

        if (*arc_clone).type_id() == TypeId::of::<CastType>() {
            let ptr = Arc::into_raw(arc_clone).cast::<CastType>();

            Some(unsafe { Arc::from_raw(ptr) })
        } else {
            None
        }
    }
}

pub trait AsAny {
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

impl<T> AsAny for T
where
    T: 'static,
{
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
