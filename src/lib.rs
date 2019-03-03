use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::fmt::Debug;

/// `InputCellID` is a unique identifier for an input cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct InputCellID(usize);
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ComputeCellID(usize);
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CallbackID(usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CellID {
    Input(InputCellID),
    Compute(ComputeCellID),
}

#[derive(Debug, PartialEq)]
pub enum RemoveCallbackError {
    NonexistentCell,
    NonexistentCallback,
}

struct InputCell<T> {
    downstreams: HashSet<CellID>,
    value: T,
}

impl<T: Copy + Debug + PartialEq> InputCell<T> {
    pub fn new(init: T) -> Self {
        InputCell {
            downstreams: HashSet::new(),
            value: init,
        }
    }
}

struct ComputeCell<'r, T: Debug> {
    fun: Box<dyn Fn(&[T]) -> T>,
    deps: Vec<CellID>,
    callbacks: Vec<RefCell<Box<dyn 'r + FnMut(T) -> ()>>>,
    prev_val: Cell<Option<T>>,
}

impl<'r, T: Copy + Debug + PartialEq + 'r> ComputeCell<'r, T> {
    pub fn new<F>(fun: F, deps: &[CellID]) -> Self
    where
        F: 'static + Fn(&[T]) -> T,
    {
        ComputeCell {
            fun: Box::new(fun),
            deps: deps.iter().map(|d| d.to_owned().clone()).collect(),
            callbacks: Vec::new(),
            prev_val: Cell::new(None),
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
                self.prev_val.set(Some(nv.clone()));
                fire_callbacks = true;
            }
        } else {
            self.prev_val.set(Some(nv.clone()));
            fire_callbacks = true;
        }

        if fire_callbacks {
            for c in self.callbacks.iter() {
                (&mut *c.borrow_mut())(nv.clone());
            }
        }

        nv
    }
}

pub struct Reactor<'r, T: Debug> {
    input_cells: Vec<InputCell<T>>,
    compute_cells: Vec<ComputeCell<'r, T>>,
}

// You are guaranteed that Reactor will only be tested against types that are Copy + PartialEq.
impl<'r, T: Copy + Debug + PartialEq + 'r> Reactor<'r, T> {
    pub fn new() -> Self {
        Reactor {
            input_cells: Vec::new(),
            compute_cells: Vec::new(),
        }
    }

    // Creates an input cell with the specified initial value, returning its ID.
    pub fn create_input(&mut self, initial: T) -> InputCellID {
        let idx = self.input_cells.len();
        let id = InputCellID(idx);
        self.input_cells.push(InputCell::new(initial));
        id
    }

    // Creates a compute cell with the specified dependencies and compute function.
    // The compute function is expected to take in its arguments in the same order as specified in
    // `dependencies`.
    // You do not need to reject compute functions that expect more arguments than there are
    // dependencies (how would you check for this, anyway?).
    //
    // If any dependency doesn't exist, returns an Err with that nonexistent dependency.
    // (If multiple dependencies do not exist, exactly which one is returned is not defined and
    // will not be tested)
    //
    // Notice thatuu there is no way to *remove* a cell.
    // This means that you may assume, without checking, that if the dependencies exist at creation
    // time they will continue to exist as long as the Reactor exists.
    pub fn create_compute<F: 'static>(
        &mut self,
        dependencies: &[CellID],
        compute_func: F,
    ) -> Result<ComputeCellID, CellID>
    where
        F: Fn(&[T]) -> T,
    {
        let cidx = self.compute_cells.len();
        let cid = ComputeCellID(cidx);

        for id in dependencies.iter() {
            match id {
                CellID::Input(InputCellID(idx)) => {
                    if !(*idx < self.input_cells.len()) {
                        return Err(*id);
                    }
                }
                CellID::Compute(ComputeCellID(idx)) => {
                    if !(*idx < self.compute_cells.len()) {
                        return Err(*id);
                    }
                }
            }
        }

        for id in dependencies.iter() {
            if let CellID::Input(InputCellID(idx)) = id {
                let _ = self.input_cells[*idx]
                    .downstreams
                    .insert(CellID::Compute(cid.clone()));
            }
        }

        let cell = ComputeCell::new(compute_func, dependencies);
        cell.call(&self); // set the initial value
        self.compute_cells.push(cell);

        Ok(cid)
    }

    // Retrieves the current value of the cell, or None if the cell does not exist.
    //
    // You may wonder whether it is possible to implement `get(&self, id: CellID) -> Option<&Cell>`
    // and have a `value(&self)` method on `Cell`.
    //
    // It turns out this introduces a significant amount of extra complexity to this exercise.
    // We chose not to cover this here, since this exercise is probably enough work as-is.
    pub fn value(&self, id: CellID) -> Option<T> {
        match id {
            CellID::Input(InputCellID(idx)) => self.input_cells.get(idx).map(|i| i.value),
            CellID::Compute(ComputeCellID(idx)) => {
                if let Some(cell) = self.compute_cells.get(idx) {
                    Some(cell.call(&self))
                } else {
                    None
                }
            }
        }
    }

    // Sets the value of the specified input cell.
    //
    // Returns false if the cell does not exist.
    //
    // Similarly, you may wonder about `get_mut(&mut self, id: CellID) -> Option<&mut Cell>`, with
    // a `set_value(&mut self, new_value: T)` method on `Cell`.
    //
    // As before, that turned out to add too much extra complexity.
    pub fn set_value(&mut self, id: InputCellID, new_value: T) -> bool {
        let InputCellID(idx) = id;
        if idx < self.input_cells.len() {
            let old_value = self.input_cells[idx].value.clone();
            if old_value == new_value {
                return true;
            }
            self.input_cells[idx].value = new_value;
            for d in self.input_cells[idx].downstreams.iter() {
                if let CellID::Compute(ComputeCellID(idx)) = d {
                    let cell = &self.compute_cells[*idx];
                    cell.call(&self);
                }
            }
            true
        } else {
            false
        }
    }

    // Adds a callback to the specified compute cell.
    //
    // Returns the ID of the just-added callback, or None if the cell doesn't exist.
    //
    // Callbacks on input cells will not be tested.
    //
    // The semantics of callbacks (as will be tested):
    // For a single set_value call, each compute cell's callbacks should each be called:
    // * Zero times if the compute cell's value did not change as a result of the set_value call.
    // * Exactly once if the compute cell's value changed as a result of the set_value call.
    //   The value passed to the callback should be the final value of the compute cell after the
    //   set_value call.
    pub fn add_callback<F: 'r + FnMut(T) -> ()>(
        &mut self,
        id: ComputeCellID,
        callback: F,
    ) -> Option<CallbackID> {
        let ComputeCellID(idx) = id;
        if !(idx < self.compute_cells.len()) {
            return None;
        }

        let cidx = self.compute_cells[idx].callbacks.len();
        let cid = CallbackID(cidx);

        self.compute_cells[idx]
            .callbacks
            .push(RefCell::new(Box::new(callback)));

        Some(cid)
    }

    // Removes the specified callback, using an ID returned from add_callback.
    //
    // Returns an Err if either the cell or callback does not exist.
    //
    // A removed callback should no longer be called.
    pub fn remove_callback(
        &mut self,
        cell: ComputeCellID,
        callback: CallbackID,
    ) -> Result<(), RemoveCallbackError> {
        unimplemented!(
            "Remove the callback identified by the CallbackID {:?} from the cell {:?}",
            callback,
            cell,
        )
    }
}
