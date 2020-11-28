use anyhow::Result;
use dragonglass_world::World;

pub enum Transition<T, E> {
    None,
    Push(Box<dyn State<T, E>>),
    Pop,
}

pub trait State<T, E> {
    fn initialize(&mut self, world: &mut World, data: &mut T, event: &E) -> Result<()> {
        Ok(())
    }

    fn finalize(&mut self, world: &mut World, data: &mut T, event: &E) -> Result<()> {
        Ok(())
    }

    fn pause(&mut self, world: &mut World, data: &mut T, event: &E) -> Result<()> {
        Ok(())
    }

    fn resume(&mut self, world: &mut World, data: &mut T, event: &E) -> Result<()> {
        Ok(())
    }

    fn update(&mut self, world: &mut World, data: &mut T, event: &E) -> Result<Transition<T, E>> {
        Ok(Transition::None)
    }
}

pub struct StateMachine<T, E> {
    states: Vec<Box<dyn State<T, E>>>,
}

impl<T, E> StateMachine<T, E> {
    pub fn new() -> Self {
        Self { states: Vec::new() }
    }

    fn current_state_action(
        &mut self,
        world: &mut World,
        data: &mut T,
        event: &E,
        action: impl Fn(&mut Box<dyn State<T, E>>, &mut World, &mut T, &E) -> Result<Transition<T, E>>,
    ) -> Result<()> {
        if let Some(state) = self.states.last_mut() {
            let transition = action(state, world, data, event)?;
            self.transition(transition, world, data, event)?;
        }
        Ok(())
    }

    pub fn initialize(&mut self, world: &mut World, data: &mut T, event: &E) -> Result<()> {
        self.current_state_action(world, data, event, |state, world, data, event| {
            state.initialize(world, data, event)?;
            Ok(Transition::None)
        })
    }

    pub fn finalize(&mut self, world: &mut World, data: &mut T, event: &E) -> Result<()> {
        self.current_state_action(world, data, event, |state, world, data, event| {
            state.finalize(world, data, event)?;
            Ok(Transition::None)
        })
    }

    pub fn update(&mut self, world: &mut World, data: &mut T, event: &E) -> Result<()> {
        self.current_state_action(world, data, event, |state, world, data, event| {
            state.update(world, data, event)
        })
    }

    pub fn transition(
        &mut self,
        transition: Transition<T, E>,
        world: &mut World,
        data: &mut T,
        event: &E,
    ) -> Result<()> {
        match transition {
            Transition::None => Ok(()),
            Transition::Push(state) => self.push(state, world, data, event),
            Transition::Pop => self.pop(world, data, event),
        }
    }

    pub fn push(
        &mut self,
        state: Box<dyn State<T, E>>,
        world: &mut World,
        data: &mut T,
        event: &E,
    ) -> Result<()> {
        self.current_state_action(world, data, event, |state, world, data, event| {
            state.pause(world, data, event)?;
            Ok(Transition::None)
        })?;
        self.states.push(state);
        self.current_state_action(world, data, event, |state, world, data, event| {
            state.initialize(world, data, event)?;
            Ok(Transition::None)
        })?;
        Ok(())
    }

    pub fn pop(&mut self, world: &mut World, data: &mut T, event: &E) -> Result<()> {
        if let Some(mut state) = self.states.pop() {
            state.finalize(world, data, event)?;
        }
        self.current_state_action(world, data, event, |state, world, data, event| {
            state.resume(world, data, event)?;
            Ok(Transition::None)
        })?;
        Ok(())
    }
}
