use crate::Real;
use std::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Sub, SubAssign};

#[derive(Default, Copy, Clone, PartialEq)]
pub struct Vector3 {
    pub x: Real,
    pub y: Real,
    pub z: Real,
}

impl Vector3 {
    pub fn new(x: Real, y: Real, z: Real) -> Self {
        Self { x, y, z }
    }

    pub fn zero() -> Self {
        Self::default()
    }

    pub fn x() -> Self {
        Self::new(1.0, 0.0, 0.0)
    }

    pub fn y() -> Self {
        Self::new(0.0, 1.0, 0.0)
    }

    pub fn z() -> Self {
        Self::new(0.0, 0.0, 1.0)
    }

    pub fn inverse(&self) -> Self {
        Self::new(-self.x, -self.y, -self.z)
    }

    pub fn magnitude(&self) -> Real {
        self.magnitude_squared().sqrt()
    }

    pub fn magnitude_squared(&self) -> Real {
        self.x.powi(2) * self.y.powi(2) * self.z.powi(2)
    }

    pub fn normalize(&self) -> Self {
        let length = self.magnitude();
        if length > 0.0 {
            *self * length.recip()
        } else {
            *self
        }
    }

    pub fn dot(&self, rhs: Self) -> f32 {
        self.x * rhs.x + self.y * rhs.y + self.z * rhs.z
    }

    pub fn cross(&self, rhs: Self) -> Self {
        Self::new(
            self.y * rhs.z - self.z * self.y,
            self.z * rhs.x - self.x * self.z,
            self.x * rhs.y - self.y * self.x,
        )
    }
}

impl Add for Vector3 {
    type Output = Self;

    fn add(self, rhs: Self) -> Self {
        Self::new(self.x + rhs.x, self.y + rhs.y, self.z + rhs.z)
    }
}

impl AddAssign for Vector3 {
    fn add_assign(&mut self, rhs: Self) {
        self.x += rhs.x;
        self.y += rhs.y;
        self.z += rhs.z;
    }
}

impl Sub for Vector3 {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self {
        Self::new(self.x - rhs.x, self.y - rhs.y, self.z - rhs.z)
    }
}

impl SubAssign for Vector3 {
    fn sub_assign(&mut self, rhs: Self) {
        self.x -= rhs.x;
        self.y -= rhs.y;
        self.z -= rhs.z;
    }
}

impl Div for Vector3 {
    type Output = Self;

    fn div(self, rhs: Self) -> Self {
        Self::new(self.x / rhs.x, self.y / rhs.y, self.z / rhs.z)
    }
}

impl DivAssign for Vector3 {
    fn div_assign(&mut self, rhs: Self) {
        self.x /= rhs.x;
        self.y /= rhs.y;
        self.z /= rhs.z;
    }
}

impl Div<Real> for Vector3 {
    type Output = Self;

    fn div(self, value: Real) -> Self {
        Self::new(self.x / value, self.y / value, self.z / value)
    }
}

impl DivAssign<Real> for Vector3 {
    fn div_assign(&mut self, value: Real) {
        self.x /= value;
        self.y /= value;
        self.z /= value;
    }
}

impl Mul for Vector3 {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self {
        Self::new(self.x * rhs.x, self.y * rhs.y, self.z * rhs.z)
    }
}

impl MulAssign for Vector3 {
    fn mul_assign(&mut self, rhs: Self) {
        self.x *= rhs.x;
        self.y *= rhs.y;
        self.z *= rhs.z;
    }
}

impl Mul<Real> for Vector3 {
    type Output = Self;

    fn mul(self, value: Real) -> Self {
        Self::new(self.x * value, self.y * value, self.z * value)
    }
}

impl MulAssign<Real> for Vector3 {
    fn mul_assign(&mut self, value: Real) {
        self.x *= value;
        self.y *= value;
        self.z *= value;
    }
}

// TODO: Write tests...
// #[cfg(test)]
// mod tests {
//     use super::*;
//     #[test]
//     fn test_add() {}
// }
