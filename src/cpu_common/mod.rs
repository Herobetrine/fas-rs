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

mod cpu_info;
mod file_handler;
mod weighting;

use std::{
    collections::HashMap,
    fs,
    sync::{atomic::AtomicIsize, OnceLock},
    time::Duration,
};

use anyhow::Result;
use cpu_info::Info;
use file_handler::FileHandler;
use libc::pid_t;
#[cfg(debug_assertions)]
use log::debug;
use log::error;

use crate::{
    api::{v1::ApiV1, v2::ApiV2, ApiV0},
    Extension,
};
use weighting::WeightedCalculator;

const BASE_FREQ: isize = 600_000;

pub static OFFSET_MAP: OnceLock<HashMap<i32, AtomicIsize>> = OnceLock::new();

#[derive(Debug)]
pub struct Controller {
    max_freq: isize,
    min_freq: isize,
    policy_freq: isize,
    cpu_infos: Vec<Info>,
    file_handler: FileHandler,
    weighted_calculator: WeightedCalculator,
}

impl Controller {
    pub fn new() -> Result<Self> {
        let cpu_infos: Vec<_> = fs::read_dir("/sys/devices/system/cpu/cpufreq")?
            .map(|entry| entry.unwrap().path())
            .filter(|path| {
                path.is_dir()
                    && path
                        .file_name()
                        .unwrap()
                        .to_str()
                        .unwrap()
                        .starts_with("policy")
            })
            .map(|path| Info::new(path).unwrap())
            .collect();

        OFFSET_MAP.get_or_init(|| {
            cpu_infos
                .iter()
                .map(|cpu| (cpu.policy, AtomicIsize::new(0)))
                .collect()
        });

        #[cfg(debug_assertions)]
        debug!("cpu infos: {cpu_infos:?}");

        let max_freq = cpu_infos
            .iter()
            .flat_map(|info| info.freqs.iter())
            .max()
            .copied()
            .unwrap();

        let min_freq = cpu_infos
            .iter()
            .flat_map(|info| info.freqs.iter())
            .min()
            .copied()
            .unwrap();

        Ok(Self {
            max_freq,
            min_freq,
            policy_freq: max_freq,
            cpu_infos,
            file_handler: FileHandler::new(),
            weighted_calculator: WeightedCalculator::new(),
        })
    }

    pub fn init_game(&mut self, extension: &Extension) {
        self.policy_freq = self.max_freq;
        extension.tigger_extentions(ApiV0::InitCpuFreq);
        extension.tigger_extentions(ApiV1::InitCpuFreq);
        extension.tigger_extentions(ApiV2::InitCpuFreq);

        for cpu in &self.cpu_infos {
            cpu.write_freq(self.max_freq, &mut self.file_handler, 1.0)
                .unwrap_or_else(|e| error!("{e:?}"));
        }
    }

    pub fn init_default(&mut self, extension: &Extension) {
        self.weighted_calculator.clear();
        self.policy_freq = self.max_freq;
        extension.tigger_extentions(ApiV0::ResetCpuFreq);
        extension.tigger_extentions(ApiV1::ResetCpuFreq);
        extension.tigger_extentions(ApiV2::ResetCpuFreq);

        for cpu in &self.cpu_infos {
            cpu.reset_freq(&mut self.file_handler)
                .unwrap_or_else(|e| error!("{e:?}"));
        }
    }

    pub fn fas_update_freq(&mut self, process: pid_t, factor: f64) {
        self.policy_freq = self
            .policy_freq
            .saturating_add((BASE_FREQ as f64 * factor) as isize)
            .clamp(self.min_freq, self.max_freq);

        #[cfg(debug_assertions)]
        {
            debug!("change freq: {}", (BASE_FREQ as f64 * factor) as isize);
            debug!("policy freq: {}", self.policy_freq);
        }

        let weights = self.weighted_calculator.update(process).unwrap();

        for policy in &self.cpu_infos {
            let weight = weights.weight(&policy.cpus).unwrap_or(1.0);
            #[cfg(debug_assertions)]
            debug!("policy{}: weight {:.2}", policy.policy, weight);
            policy
                .write_freq(self.policy_freq, &mut self.file_handler, weight)
                .unwrap_or_else(|e| error!("{e:?}"));
        }
    }

    pub fn scale_factor(target_fps: u32, frame: Duration, target: Duration) -> f64 {
        if frame > target {
            let factor_a = (frame - target).as_nanos() as f64 / target.as_nanos() as f64;
            let factor_b = 120.0 / f64::from(target_fps);
            factor_a * factor_b
        } else {
            let factor_a = (target - frame).as_nanos() as f64 / target.as_nanos() as f64;
            let factor_b = 120.0 / f64::from(target_fps);
            factor_a * factor_b * -1.0
        }
    }
}
