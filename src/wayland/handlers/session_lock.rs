// SPDX-License-Identifier: GPL-3.0-only

use crate::state::{SessionLock, State};
use smithay::{
    delegate_session_lock,
    output::Output,
    reexports::wayland_server::protocol::wl_output::WlOutput,
    wayland::session_lock::{
        surface::LockSurface, SessionLockHandler, SessionLockManagerState, SessionLocker,
    },
};
use std::collections::HashMap;

impl SessionLockHandler for State {
    fn lock_state(&mut self) -> &mut SessionLockManagerState {
        &mut self.common.session_lock_manager_state
    }

    fn lock(&mut self, locker: SessionLocker) {
        // XXX can there already be a lock?
        locker.lock();
        self.common.session_lock = Some(SessionLock {
            surfaces: HashMap::new(),
        })
    }

    fn unlock(&mut self) {
        self.common.session_lock = None;
    }

    fn new_surface(&mut self, lock_surface: LockSurface, wl_output: WlOutput) {
        if let Some(session_lock) = &mut self.common.session_lock {
            if let Some(output) = Output::from_resource(&wl_output) {
                session_lock.surfaces.insert(output, lock_surface);
            }
        }
    }
}

delegate_session_lock!(State);
