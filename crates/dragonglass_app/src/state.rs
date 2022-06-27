use anyhow::{Context, Result};
use dragonglass_world::World;
use std::path::Path;
use winit::event::{ElementState, Event, KeyboardInput, MouseButton};

use crate::Resources;

pub trait State {
    fn label(&self) -> String {
        "Unlabeled Game State".to_string()
    }

    fn world(&mut self) -> Option<&mut World> {
        None
    }

    fn on_start(&mut self, _resources: &mut Resources) -> Result<()> {
        Ok(())
    }

    fn on_stop(&mut self, _resources: &mut Resources) -> Result<()> {
        Ok(())
    }

    fn on_pause(&mut self, _resources: &mut Resources) -> Result<()> {
        Ok(())
    }

    fn on_resume(&mut self, _resources: &mut Resources) -> Result<()> {
        Ok(())
    }

    fn update(&mut self, _resources: &mut Resources) -> Result<Transition> {
        Ok(Transition::None)
    }

    fn update_gui(&mut self, _resources: &mut Resources) -> Result<Transition> {
        Ok(Transition::None)
    }

    // fn on_gamepad_event(
    //     &mut self,
    //     _resources: &mut Resources,
    //     _event: GilrsEvent,
    // ) -> Result<Transition> {
    //     Ok(Transition::None)
    // }

    fn on_file_dropped(&mut self, _resources: &mut Resources, _path: &Path) -> Result<Transition> {
        Ok(Transition::None)
    }

    fn on_mouse(
        &mut self,
        _resources: &mut Resources,
        _button: &MouseButton,
        _button_state: &ElementState,
    ) -> Result<Transition> {
        Ok(Transition::None)
    }

    fn on_key(&mut self, _resources: &mut Resources, _input: KeyboardInput) -> Result<Transition> {
        Ok(Transition::None)
    }

    fn on_event(&mut self, _resources: &mut Resources, _event: &Event<()>) -> Result<Transition> {
        Ok(Transition::None)
    }
}

pub enum Transition {
    None,
    Pop,
    Push(Box<dyn State>),
    Switch(Box<dyn State>),
    Quit,
}

pub struct StateMachine {
    running: bool,
    states: Vec<Box<dyn State>>,
}

impl StateMachine {
    pub fn new(initial_state: impl State + 'static) -> Self {
        Self {
            running: false,
            states: vec![Box::new(initial_state)],
        }
    }

    pub fn world(&mut self) -> Result<Option<&mut World>> {
        Ok(self.active_state_mut()?.world())
    }

    pub fn active_state_label(&self) -> Option<String> {
        if !self.running {
            return None;
        }
        self.states.last().map(|state| state.label())
    }

    pub fn is_running(&self) -> bool {
        self.running
    }

    pub fn start(&mut self, resources: &mut Resources) -> Result<()> {
        if self.running {
            return Ok(());
        }
        self.running = true;
        self.active_state_mut()?.on_start(resources)
    }

    pub fn handle_event(&mut self, resources: &mut Resources, event: &Event<()>) -> Result<()> {
        if !self.running {
            return Ok(());
        }
        let transition = self.active_state_mut()?.on_event(resources, event)?;
        self.transition(transition, resources)
    }

    pub fn update(&mut self, resources: &mut Resources) -> Result<()> {
        if !self.running {
            return Ok(());
        }
        let transition = self.active_state_mut()?.update(resources)?;
        self.transition(transition, resources)
    }

    pub fn update_gui(&mut self, resources: &mut Resources) -> Result<()> {
        if !self.running {
            return Ok(());
        }
        let transition = self.active_state_mut()?.update_gui(resources)?;
        self.transition(transition, resources)
    }

    // pub fn on_gamepad_event(&mut self, resources: &mut Resources, event: GilrsEvent) -> Result<()> {
    //     if !self.running {
    //         return Ok(());
    //     }
    //     let transition = self
    //         .active_state_mut()?
    //         .on_gamepad_event(resources, event)?;
    //     self.transition(transition, resources)
    // }

    pub fn on_file_dropped(&mut self, resources: &mut Resources, path: &Path) -> Result<()> {
        if !self.running {
            return Ok(());
        }
        let transition = self.active_state_mut()?.on_file_dropped(resources, path)?;
        self.transition(transition, resources)
    }

    pub fn on_mouse(
        &mut self,
        resources: &mut Resources,
        button: &MouseButton,
        button_state: &ElementState,
    ) -> Result<()> {
        if !self.running {
            return Ok(());
        }
        let transition = self
            .active_state_mut()?
            .on_mouse(resources, button, button_state)?;
        self.transition(transition, resources)
    }

    pub fn on_key(&mut self, resources: &mut Resources, input: KeyboardInput) -> Result<()> {
        if !self.running {
            return Ok(());
        }
        let transition = self.active_state_mut()?.on_key(resources, input)?;
        self.transition(transition, resources)
    }

    pub fn on_event(&mut self, resources: &mut Resources, event: &Event<()>) -> Result<()> {
        if !self.running {
            return Ok(());
        }
        let transition = self.active_state_mut()?.on_event(resources, event)?;
        self.transition(transition, resources)
    }

    fn transition(&mut self, request: Transition, resources: &mut Resources) -> Result<()> {
        if !self.running {
            return Ok(());
        }
        match request {
            Transition::None => Ok(()),
            Transition::Pop => self.pop(resources),
            Transition::Push(state) => self.push(state, resources),
            Transition::Switch(state) => self.switch(state, resources),
            Transition::Quit => self.stop(resources),
        }
    }

    fn active_state_mut(&mut self) -> Result<&mut Box<(dyn State + 'static)>> {
        self.states
            .last_mut()
            .context("Tried to access state in state machine with no states present!")
    }

    fn switch(&mut self, state: Box<dyn State>, resources: &mut Resources) -> Result<()> {
        if !self.running {
            return Ok(());
        }
        if let Some(mut state) = self.states.pop() {
            state.on_stop(resources)?;
        }
        self.states.push(state);
        self.active_state_mut()?.on_start(resources)
    }

    fn push(&mut self, state: Box<dyn State>, resources: &mut Resources) -> Result<()> {
        if !self.running {
            return Ok(());
        }
        if let Ok(state) = self.active_state_mut() {
            state.on_pause(resources)?;
        }
        self.states.push(state);
        self.active_state_mut()?.on_start(resources)
    }

    fn pop(&mut self, resources: &mut Resources) -> Result<()> {
        if !self.running {
            return Ok(());
        }

        if let Some(mut state) = self.states.pop() {
            state.on_stop(resources)?;
        }

        if let Some(state) = self.states.last_mut() {
            state.on_resume(resources)
        } else {
            self.running = false;
            Ok(())
        }
    }

    pub fn stop(&mut self, resources: &mut Resources) -> Result<()> {
        if !self.running {
            return Ok(());
        }
        while let Some(mut state) = self.states.pop() {
            state.on_stop(resources)?;
        }
        self.running = false;
        Ok(())
    }
}
