use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::collections::HashSet;
use std::convert::From;
use std::fmt::Debug;

/// `ComputeCellID` is a unique identifier for a compute cell.
/// Values of type `InputCellID` and `ComputeCellID` should not be mutually assignable,
/// demonstrated by the following tests:
///
/// ```compile_fail
/// let mut r = react::Reactor::new();
/// let input: react::ComputeCellID = r.create_input(111);
/// ```
///
/// ```compile_fail
/// let mut r = react::Reactor::new();
/// let input = r.create_input(111);
/// let compute: react::InputCellID = r.create_compute(&[react::CellID::Input(input)], |_| 222).unwrap();
/// ```
use crate::Reactor;

/// `InputCellID` is a unique identifier for an input cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct InputCellID(pub usize);

impl From<usize> for InputCellID {
    fn from(f: usize) -> Self {
        InputCellID(f)
    }
}

impl From<usize> for ComputeCellID {
    fn from(f: usize) -> Self {
        ComputeCellID(f)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CellID {
    Input(InputCellID),
    Compute(ComputeCellID),
}

pub struct InputCell<T> {
    pub clients: HashSet<ComputeCellID>,
    pub value: T,
}

impl<T: Copy + Debug + PartialEq> InputCell<T> {
    pub fn new(init: T) -> Self {
        InputCell {
            clients: HashSet::new(),
            value: init,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ComputeCellID(pub usize);
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CallbackID(pub usize);

pub type Callback<'reactor, T> = RefCell<Box<dyn 'reactor + FnMut(T)>>;

pub struct ComputeCell<'r, T: Debug> {
    fun: Box<dyn 'r + Fn(&[T]) -> T>,
    deps: Vec<CellID>,
    pub callbacks: HashMap<CallbackID, Callback<'r, T>>,
    prev_val: Cell<Option<T>>,
    pub next_cbid: usize, // increases monotonically; increments on adding a callback
    pub clients: HashSet<ComputeCellID>,
}

impl<'r, T: Copy + Debug + PartialEq + 'r> ComputeCell<'r, T> {
    pub fn new<F>(fun: F, deps: &[CellID]) -> Self
    where
        F: 'r + Fn(&[T]) -> T,
    {
        ComputeCell {
            fun: Box::new(fun),
            deps: deps.to_vec(),
            callbacks: HashMap::new(),
            prev_val: Cell::new(None),
            next_cbid: 0,
            clients: HashSet::new(),
        }
    }

    pub fn call(&self, reactor: &Reactor<'r, T>) -> T {
        let deps = self
            .deps
            .iter()
            .map(|c| reactor.value(*c).unwrap())
            .collect::<Vec<T>>();
        let nv = (self.fun)(&deps);

        let mut fire_callbacks = false;

        if let Some(pv) = self.prev_val.get() {
            if nv != pv {
                self.prev_val.set(Some(nv));
                fire_callbacks = true;
            }
        } else {
            self.prev_val.set(Some(nv));
            fire_callbacks = true;
        }

        if fire_callbacks {
            for c in self.callbacks.values() {
                (&mut *c.borrow_mut())(nv);
            }
        }

        nv
    }
}
