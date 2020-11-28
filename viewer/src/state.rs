use anyhow::Result;
use dragonglass_world::World;

pub enum Transition<T> {
    None,
    Push(Box<dyn State<T>>),
    Pop,
}

pub trait State<T> {
    fn initialize(&mut self, world: &mut World, data: &mut T) -> Result<()> {
        Ok(())
    }

    fn finalize(&mut self, world: &mut World, data: &mut T) -> Result<()> {
        Ok(())
    }

    fn pause(&mut self, world: &mut World, data: &mut T) -> Result<()> {
        Ok(())
    }

    fn resume(&mut self, world: &mut World, data: &mut T) -> Result<()> {
        Ok(())
    }

    fn update(&mut self, world: &mut World, data: &mut T) -> Result<Transition<T>> {
        Ok(Transition::None)
    }
}

pub struct StateMachine<T> {
    states: Vec<Box<dyn State<T>>>,
}

impl<T> StateMachine<T> {
    pub fn new() -> Self {
        Self { states: Vec::new() }
    }

    fn current_state_action(
        &mut self,
        world: &mut World,
        data: &mut T,
        action: impl Fn(&mut Box<dyn State<T>>, &mut World, &mut T) -> Result<Transition<T>>,
    ) -> Result<()> {
        if let Some(state) = self.states.last_mut() {
            let transition = action(state, world, data)?;
            self.transition(transition, world, data)?;
        }
        Ok(())
    }

    pub fn initialize(&mut self, world: &mut World, data: &mut T) -> Result<()> {
        self.current_state_action(world, data, |state, world, data| {
            state.initialize(world, data)?;
            Ok(Transition::None)
        })
    }

    pub fn finalize(&mut self, world: &mut World, data: &mut T) -> Result<()> {
        self.current_state_action(world, data, |state, world, data| {
            state.finalize(world, data)?;
            Ok(Transition::None)
        })
    }

    pub fn update(&mut self, world: &mut World, data: &mut T) -> Result<()> {
        self.current_state_action(world, data, |state, world, data| state.update(world, data))
    }

    pub fn transition(
        &mut self,
        transition: Transition<T>,
        world: &mut World,
        data: &mut T,
    ) -> Result<()> {
        match transition {
            Transition::None => Ok(()),
            Transition::Push(state) => self.push(state, world, data),
            Transition::Pop => self.pop(world, data),
        }
    }

    pub fn push(
        &mut self,
        state: Box<dyn State<T>>,
        world: &mut World,
        data: &mut T,
    ) -> Result<()> {
        self.current_state_action(world, data, |state, world, data| {
            state.pause(world, data)?;
            Ok(Transition::None)
        })?;
        self.states.push(state);
        self.current_state_action(world, data, |state, world, data| {
            state.initialize(world, data)?;
            Ok(Transition::None)
        })?;
        Ok(())
    }

    pub fn pop(&mut self, world: &mut World, data: &mut T) -> Result<()> {
        if let Some(mut state) = self.states.pop() {
            state.finalize(world, data)?;
        }
        self.current_state_action(world, data, |state, world, data| {
            state.resume(world, data)?;
            Ok(Transition::None)
        })?;
        Ok(())
    }
}
