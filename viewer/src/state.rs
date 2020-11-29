use anyhow::Result;
use dragonglass_world::World;

pub struct StateData<'a, T, E> {
    world: &'a World,
    event: &'a E,
    data: &'a T,
}

impl<'a, T, E> StateData<'a, T, E> {
    pub fn new(world: &'a World, event: &'a E, data: &'a T) -> Self {
        Self { world, event, data }
    }

    pub fn transfer(&self) -> Self {
        let Self { world, event, data } = self;
        Self { world, event, data }
    }
}

pub enum Transition<T, E> {
    None,
    Push(Box<dyn State<T, E>>),
    Pop,
}

pub trait State<T, E> {
    fn initialize(&mut self, data: StateData<'_, T, E>) -> Result<Transition<T, E>> {
        Ok(Transition::None)
    }

    fn finalize(&mut self, data: StateData<'_, T, E>) -> Result<Transition<T, E>> {
        Ok(Transition::None)
    }

    fn pause(&mut self, data: StateData<'_, T, E>) -> Result<Transition<T, E>> {
        Ok(Transition::None)
    }

    fn resume(&mut self, data: StateData<'_, T, E>) -> Result<Transition<T, E>> {
        Ok(Transition::None)
    }

    fn update(&mut self, data: StateData<'_, T, E>) -> Result<Transition<T, E>> {
        Ok(Transition::None)
    }
}

pub struct StateMachine<'a, T, E> {
    data: StateData<'a, T, E>,
    states: Vec<Box<dyn State<T, E>>>,
}

impl<'a, T, E> StateMachine<'a, T, E> {
    pub fn new(data: StateData<'a, T, E>) -> Self {
        Self {
            data,
            states: Vec::new(),
        }
    }

    pub fn initialize(&mut self) -> Result<()> {
        self.action(|state, data| state.initialize(data))
    }

    pub fn finalize(&mut self, data: StateData<'_, T, E>) -> Result<()> {
        self.action(|state, data| state.finalize(data))
    }

    pub fn update(&mut self) -> Result<()> {
        self.action(|state, data| state.update(data))
    }

    fn action(
        &mut self,
        action: impl FnOnce(&mut Box<dyn State<T, E>>, StateData<'a, T, E>) -> Result<Transition<T, E>>,
    ) -> Result<()> {
        if let Some(state) = self.states.last_mut() {
            let transition = action(state, self.data.transfer())?;
            self.transition(transition)?;
        }
        Ok(())
    }

    pub fn transition(&mut self, transition: Transition<T, E>) -> Result<()> {
        match transition {
            Transition::None => Ok(()),
            Transition::Push(state) => self.push(state),
            Transition::Pop => self.pop(),
        }
    }

    pub fn push(&mut self, state: Box<dyn State<T, E>>) -> Result<()> {
        self.action(|state, data| state.pause(data))?;
        self.states.push(state);
        self.action(|state, data| state.initialize(data))?;
        Ok(())
    }

    pub fn pop(&mut self) -> Result<()> {
        self.action(|state, data| state.finalize(data))?;
        self.states.pop();
        self.action(|state, data| state.resume(data))?;
        Ok(())
    }
}
