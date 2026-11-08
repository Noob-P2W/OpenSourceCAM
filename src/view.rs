//! Orthographic orbit camera, independent of the desktop renderer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub yaw: f64,
    pub pitch: f64,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            yaw: std::f64::consts::FRAC_PI_4,
            pitch: 35_f64.to_radians(),
        }
    }
}

impl Camera {
    /// Column-major view basis, for the native model shader.
    pub fn matrix(self) -> [f32; 9] {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        [
            cy,
            -sy * sp,
            -sy * cp,
            -sy,
            -cy * sp,
            -cy * cp,
            0.0,
            -cp,
            sp,
        ]
        .map(|v| v as f32)
    }
    pub fn top() -> Self {
        Self {
            yaw: 0.0,
            pitch: std::f64::consts::FRAC_PI_2,
        }
    }

    pub fn orbit(&mut self, dx: f64, dy: f64) {
        self.yaw = (self.yaw + dx * 0.008).rem_euclid(std::f64::consts::TAU);
        self.pitch = (self.pitch + dy * 0.008).clamp(-1.48, 1.48);
    }

    /// Screen X, screen Y (down), depth (larger is nearer the camera).
    pub fn project(self, point: [f64; 3]) -> [f64; 3] {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        let along = point[0] * sy + point[1] * cy;
        [
            point[0] * cy - point[1] * sy,
            -along * sp - point[2] * cp,
            -along * cp + point[2] * sp,
        ]
    }
}
