use std::path::PathBuf;

use anyhow::{Context, Result};
use winit::event::{ElementState, Event, KeyboardInput, MouseButton};

use crate::Resources;

pub trait State {
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

    fn gui_active(&mut self) -> bool {
        return false;
    }

    fn update_gui(&mut self, _resources: &mut Resources) -> Result<()> {
        Ok(())
    }

    fn on_file_dropped(&mut self, _resources: &mut Resources, _path: &PathBuf) -> Result<()> {
        Ok(())
    }

    fn on_mouse(
        &mut self,
        _resources: &mut Resources,
        _button: &MouseButton,
        _button_state: &ElementState,
    ) -> Result<()> {
        Ok(())
    }

    fn on_key(&mut self, _resources: &mut Resources, _input: KeyboardInput) -> Result<()> {
        Ok(())
    }

    fn handle_event(
        &mut self,
        _resources: &mut Resources,
        _event: &Event<()>,
    ) -> Result<Transition> {
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

    pub fn is_running(&self) -> bool {
        self.running
    }

    pub fn gui_active(&mut self) -> Result<bool> {
        Ok(self.active_state()?.gui_active())
    }

    pub fn on_file_dropped(&mut self, resources: &mut Resources, path: &PathBuf) -> Result<()> {
        self.active_state()?.on_file_dropped(resources, path)
    }

    pub fn on_mouse(
        &mut self,
        resources: &mut Resources,
        button: &MouseButton,
        button_state: &ElementState,
    ) -> Result<()> {
        self.active_state()?
            .on_mouse(resources, button, button_state)
    }

    pub fn on_key(&mut self, resources: &mut Resources, input: KeyboardInput) -> Result<()> {
        self.active_state()?.on_key(resources, input)
    }

    fn active_state(&mut self) -> Result<&mut Box<dyn State>> {
        let state = self
            .states
            .last_mut()
            .context("No state is available in the state machine!")?;
        Ok(state)
    }

    pub fn start(&mut self, resources: &mut Resources) -> Result<()> {
        if !self.running {
            let state = self
                .states
                .last_mut()
                .context("Tried to start state machine with no states present!")?;
            state.on_start(resources)?;
            self.running = true;
        }
        Ok(())
    }

    pub fn handle_event(&mut self, resources: &mut Resources, event: &Event<()>) -> Result<()> {
        if self.running {
            let transition = match self.states.last_mut() {
                Some(state) => state.handle_event(resources, &event)?,
                None => Transition::None,
            };
            self.transition(transition, resources)?;
        }
        Ok(())
    }

    pub fn update(&mut self, resources: &mut Resources) -> Result<()> {
        if self.running {
            let transition = match self.states.last_mut() {
                Some(state) => {
                    if state.gui_active() {
                        state.update_gui(resources)?;
                    }
                    state.update(resources)?
                }
                None => Transition::None,
            };
            self.transition(transition, resources)?;
        }
        Ok(())
    }

    pub fn transition(&mut self, request: Transition, resources: &mut Resources) -> Result<()> {
        if self.running {
            match request {
                Transition::None => (),
                Transition::Pop => self.pop(resources)?,
                Transition::Push(state) => self.push(state, resources)?,
                Transition::Switch(state) => self.switch(state, resources)?,
                Transition::Quit => self.stop(resources)?,
            }
        }
        Ok(())
    }

    fn switch(&mut self, state: Box<dyn State>, resources: &mut Resources) -> Result<()> {
        if self.running {
            if let Some(mut state) = self.states.pop() {
                state.on_stop(resources)?;
            }
            self.states.push(state);
            let new_state = self.states.last_mut().unwrap();
            new_state.on_start(resources)?;
        }
        Ok(())
    }

    fn push(&mut self, state: Box<dyn State>, resources: &mut Resources) -> Result<()> {
        if self.running {
            if let Some(state) = self.states.last_mut() {
                state.on_pause(resources)?;
            }
            self.states.push(state);
            let new_state = self.states.last_mut().unwrap();
            new_state.on_start(resources)?;
        }
        Ok(())
    }

    fn pop(&mut self, resources: &mut Resources) -> Result<()> {
        if self.running {
            if let Some(mut state) = self.states.pop() {
                state.on_stop(resources)?;
            }
            if let Some(state) = self.states.last_mut() {
                state.on_resume(resources)?;
            } else {
                self.running = false;
            }
        }
        Ok(())
    }

    pub fn stop(&mut self, resources: &mut Resources) -> Result<()> {
        if self.running {
            while let Some(mut state) = self.states.pop() {
                state.on_stop(resources)?;
            }
            self.running = false;
        }
        Ok(())
    }
}
