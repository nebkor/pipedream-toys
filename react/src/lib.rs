use std::cell::RefCell;
use std::collections::HashSet;
use std::fmt::Debug;

mod reactive_cells;
pub use reactive_cells::*;

#[derive(Debug, PartialEq)]
pub enum RemoveCallbackError {
    NonexistentCell,
    NonexistentCallback,
}

#[derive(Default)]
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
        self.input_cells.push(InputCell::new(initial));
        idx.into()
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
    // Notice that there is no way to *remove* a cell.
    // This means that you may assume, without checking, that if the dependencies exist at creation
    // time they will continue to exist as long as the Reactor exists.
    pub fn create_compute<F>(
        &mut self,
        dependencies: &[CellID],
        compute_func: F,
    ) -> Result<ComputeCellID, CellID>
    where
        F: 'r + Fn(&[T]) -> T,
    {
        let cidx = self.compute_cells.len();
        let cid = ComputeCellID(cidx);

        for id in dependencies.iter() {
            match id {
                CellID::Input(InputCellID(idx)) => {
                    if *idx >= self.input_cells.len() {
                        return Err(*id);
                    }
                }
                CellID::Compute(ComputeCellID(idx)) => {
                    if *idx >= self.compute_cells.len() {
                        return Err(*id);
                    }
                }
            }
        }

        // register as clients with all dependencies.
        for id in dependencies.iter() {
            match id {
                CellID::Input(InputCellID(idx)) => {
                    let _ = self.input_cells[*idx].clients.insert(cid);
                }
                CellID::Compute(ComputeCellID(idx)) => {
                    let _ = self.compute_cells[*idx].clients.insert(cid);
                }
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
            let old_value = self.input_cells[idx].value;
            if old_value == new_value {
                return true;
            }
            self.input_cells[idx].value = new_value;

            let mut clients1 = self.input_cells[idx].clients.clone();
            let mut clients2 = HashSet::new();

            let mut done = false;

            // Recursively iterate through all clients until we've converged on the
            // the stable set of them. Does at least N extra checks, where N is
            // the numer of ultimate clients.
            while !done {
                for client in clients1.iter() {
                    clients2.insert(client.clone());
                    let ComputeCellID(idx) = client;
                    let cell = &self.compute_cells[*idx];
                    // first find all the clients that will be called without us
                    clients2.extend(cell.clients.iter());
                }
                for client in clients2.iter() {
                    let ComputeCellID(idx) = client;
                    let cell = &self.compute_cells[*idx];
                    clients1.extend(cell.clients.iter());
                }

                done = clients1 == clients2;
            }

            // This has the potential to call more clients than needed, but ComputeCells
            // cache their previous value and only invoke their callbacks on change,
            // so client callbacks won't get invoked more than once.
            //
            // There's an implicit assumption here that each ComputeCell's function is
            // cheap to run, which is probably not true in general. We could do a
            // topological sort of the client graph to ensure we only call leaf nodes.
            for client in clients1 {
                let ComputeCellID(idx) = client;
                let cell = &self.compute_cells[idx];
                cell.call(&self);
            }
            // we have set a new value and called all clients, return true
            true
        } else {
            // the new value was the same as the old value, return false
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
        if idx >= self.compute_cells.len() {
            return None;
        }

        let cidx = self.compute_cells[idx].next_cbid.to_owned();
        self.compute_cells[idx].next_cbid += 1;
        let cid = CallbackID(cidx);

        self.compute_cells[idx]
            .callbacks
            .insert(cid, RefCell::new(Box::new(callback)));

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
        let ComputeCellID(idx) = cell;
        if let Some(compute_cell) = self.compute_cells.get_mut(idx) {
            if compute_cell.callbacks.remove(&callback).is_some() {
                return Ok(());
            } else {
                return Err(RemoveCallbackError::NonexistentCallback);
            }
        } else {
            Err(RemoveCallbackError::NonexistentCell)
        }
    }
}
