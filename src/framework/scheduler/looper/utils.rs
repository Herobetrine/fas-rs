// Copyright 2023 shadow3aaa@gitbub.com
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::time::{Duration, Instant};

use log::info;

use super::{super::FasData, buffer::BufferState, Buffer, Looper, State};
use crate::{
    api::{v1::ApiV1, v2::ApiV2},
    framework::{api::ApiV0, utils::get_process_name},
};

const DELAY_TIME: Duration = Duration::from_secs(3);

impl Looper {
    pub fn retain_topapp(&mut self) {
        if let Some(buffer) = self.buffer.as_ref() {
            if !self.windows_watcher.topapp_pids().contains(&buffer.pid) {
                #[cfg(feature = "use_ebpf")]
                let _ = self.analyzer.detach_app(buffer.pid);
                let pkg = buffer.pkg.clone();
                self.extension
                    .tigger_extentions(ApiV0::UnloadFas(buffer.pid, pkg.clone()));
                self.extension
                    .tigger_extentions(ApiV1::UnloadFas(buffer.pid, pkg.clone()));
                self.extension
                    .tigger_extentions(ApiV2::UnloadFas(buffer.pid, pkg));
                self.buffer = None;
            }
        }

        if self.buffer.is_none() {
            self.disable_fas();
        } else {
            self.enable_fas();
        }
    }

    pub fn disable_fas(&mut self) {
        match self.state {
            State::Working => {
                self.state = State::NotWorking;
                self.cleaner.undo_cleanup();
                self.controller.init_default(&self.extension);
                self.extension.tigger_extentions(ApiV0::StopFas);
                self.extension.tigger_extentions(ApiV1::StopFas);
                self.extension.tigger_extentions(ApiV2::StopFas);
            }
            State::Waiting => self.state = State::NotWorking,
            State::NotWorking => (),
        }
    }

    pub fn enable_fas(&mut self) {
        match self.state {
            State::NotWorking => {
                self.state = State::Waiting;
                self.delay_timer = Instant::now();
                self.extension.tigger_extentions(ApiV0::StartFas);
                self.extension.tigger_extentions(ApiV1::StartFas);
                self.extension.tigger_extentions(ApiV2::StartFas);
            }
            State::Waiting => {
                if self.delay_timer.elapsed() > DELAY_TIME {
                    self.state = State::Working;
                    self.cleaner.cleanup();
                    self.controller.init_game(&self.extension);
                }
            }
            State::Working => (),
        }
    }

    pub fn buffer_update(&mut self, d: &FasData) -> Option<BufferState> {
        if !self.windows_watcher.topapp_pids().contains(&d.pid) || d.frametime.is_zero() {
            return None;
        }

        let pid = d.pid;
        let frametime = d.frametime;

        if let Some(buffer) = self.buffer.as_mut() {
            buffer.push_frametime(frametime, &self.extension);
            Some(buffer.state)
        } else {
            let Ok(pkg) = get_process_name(d.pid) else {
                return None;
            };
            let target_fps = self.config.target_fps(&pkg)?;

            info!("New fas buffer on: [{pkg}]");

            self.extension
                .tigger_extentions(ApiV0::LoadFas(pid, pkg.clone()));
            self.extension
                .tigger_extentions(ApiV1::LoadFas(pid, pkg.clone()));
            self.extension
                .tigger_extentions(ApiV2::LoadFas(pid, pkg.clone()));

            let mut buffer = Buffer::new(target_fps, pid, pkg);
            buffer.push_frametime(frametime, &self.extension);

            self.buffer = Some(buffer);

            Some(BufferState::Unusable)
        }
    }
}
