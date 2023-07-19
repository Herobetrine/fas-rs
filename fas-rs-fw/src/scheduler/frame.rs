use std::{error::Error, time::Duration};

use likely_stable::unlikely;

use super::Scheduler;
use crate::{debug, Fps, FrameTime, TargetFps, VirtualFrameSensor, VirtualPerformanceController};

impl Scheduler {
    pub(super) fn process_unload(
        sensor: &dyn VirtualFrameSensor,
        controller: &dyn VirtualPerformanceController,
    ) -> Result<(), Box<dyn Error>> {
        sensor.pause()?;
        controller.plug_out()?;
        Ok(())
    }

    pub(super) fn init_load(
        sensor: &dyn VirtualFrameSensor,
        controller: &dyn VirtualPerformanceController,
        frame_windows: u32,
    ) -> Result<Duration, Box<dyn Error>> {
        let fps_time = Duration::from_millis((frame_windows * 40).into());
        sensor.resume(frame_windows, fps_time)?;
        controller.plug_in()?;
        Ok(fps_time)
    }

    pub(super) fn process_load(
        sensor: &dyn VirtualFrameSensor,
        controller: &dyn VirtualPerformanceController,
        target_fps: TargetFps,
    ) {
        let frametimes = sensor.frametimes(target_fps);
        let fps = sensor.fps();

        if unlikely(jank(&frametimes, fps, target_fps)) {
            controller.release();
        } else {
            controller.limit();
        }
    }
}

// 判断是否出现卡顿
fn jank(frametime: &[FrameTime], avg_fps: Fps, target_fps: TargetFps) -> bool {
    debug! {
        println!("avg fps: {}", avg_fps);
        println!("frametime: {:?}", frametime.iter().max());
    }

    let target_frametime = Duration::from_secs(1) / target_fps;
    frametime.is_empty()
        || avg_fps <= target_fps - 3
        || frametime.iter().any(|ft| *ft > target_frametime * 30 / 29)
}
