use rocket::serde::{Deserialize, Serialize};
use std::ops::{Add, AddAssign, Mul, Sub, SubAssign};

#[derive(Debug, Clone, Copy, Deserialize, Serialize, Eq, PartialEq, Default)]
#[serde(crate = "rocket::serde", rename_all = "camelCase")]
pub struct Vec2(pub i8, pub i8);

impl Vec2 {
	pub fn is_inside_board(&self) -> bool {
		self.0 >= 0 && self.1 >= 0 && self.0 < 8 && self.1 < 8
	}
}

impl Add<Vec2> for Vec2 {
	type Output = Vec2;

	fn add(self, rhs: Vec2) -> Self::Output {
		Vec2(self.0 + rhs.0, self.1 + rhs.1)
	}
}

impl Sub<Vec2> for Vec2 {
	type Output = Vec2;

	fn sub(self, rhs: Vec2) -> Self::Output {
		Vec2(self.0 - rhs.0, self.1 - rhs.1)
	}
}

impl AddAssign<&Vec2> for Vec2 {
	fn add_assign(&mut self, rhs: &Vec2) {
		self.0 += rhs.0;
		self.1 += rhs.1;
	}
}

impl SubAssign<&Vec2> for Vec2 {
	fn sub_assign(&mut self, rhs: &Vec2) {
		self.0 -= rhs.0;
		self.1 -= rhs.1;
	}
}

impl Mul<i8> for Vec2 {
	type Output = Vec2;

	fn mul(self, rhs: i8) -> Self::Output {
		Vec2(self.0 * rhs, self.1 * rhs)
	}
}

impl Mul<Vec2> for i8 {
	type Output = Vec2;

	fn mul(self, rhs: Vec2) -> Self::Output {
		Vec2(self * rhs.0, self * rhs.1)
	}
}
