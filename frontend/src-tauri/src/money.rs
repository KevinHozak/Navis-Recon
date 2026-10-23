use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use std::{
    fmt,
    ops::{Add, AddAssign, Neg, Sub, SubAssign},
    str::FromStr,
};

/// Fixed scale decimal money. Means and percentages use round-half-away-from-zero
/// at the 18th decimal place; division by integers is exact when representable.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct Money(i128);

impl Money {
    pub(crate) const SCALE: i128 = 1_000_000_000_000_000_000;
    pub(crate) const MAX: i128 = 1_000_000_000_000_000_i128 * Self::SCALE;
    pub(crate) fn from_scaled(v: i128) -> Result<Self, String> {
        if v.unsigned_abs() > Self::MAX as u128 {
            Err("amount exceeds supported 1e15 bound".into())
        } else {
            Ok(Self(v))
        }
    }
    pub(crate) fn parse(s: &str) -> Result<Self, String> {
        if s.is_empty() {
            return Err("empty decimal".into());
        }
        let (neg, body) = if let Some(v) = s.strip_prefix('-') {
            (true, v)
        } else if let Some(v) = s.strip_prefix('+') {
            (false, v)
        } else {
            (false, s)
        };
        let mut parts = body.split('.');
        let whole = parts.next().unwrap();
        let frac = parts.next().unwrap_or("");
        if parts.next().is_some()
            || (whole.is_empty() && frac.is_empty())
            || !whole.bytes().all(|b| b.is_ascii_digit())
            || !frac.bytes().all(|b| b.is_ascii_digit())
        {
            return Err("invalid plain decimal".into());
        }
        if frac.len() > 18 {
            return Err("decimal precision exceeds 18 places".into());
        }
        let w = if whole.is_empty() {
            0
        } else {
            whole.parse::<i128>().map_err(|_| "decimal out of range")?
        };
        let f = if frac.is_empty() {
            0
        } else {
            frac.parse::<i128>().map_err(|_| "invalid fraction")?
                * 10_i128.pow((18 - frac.len()) as u32)
        };
        let mut n = w
            .checked_mul(Self::SCALE)
            .and_then(|x| x.checked_add(f))
            .ok_or("decimal out of range")?;
        if neg {
            n = n.checked_neg().ok_or("decimal out of range")?;
        }
        Self::from_scaled(n)
    }
    pub(crate) fn is_zero(self) -> bool {
        self.0 == 0
    }
    pub(crate) fn abs(self) -> Self {
        Self(self.0.abs())
    }
    pub(crate) fn checked_add(self, rhs: Self) -> Option<Self> {
        self.0.checked_add(rhs.0).map(Self)
    }
    pub(crate) fn checked_div(self, d: i128) -> Option<Self> {
        if d == 0 {
            return None;
        }
        let q = self.0 / d;
        let r = self.0 % d;
        if r == 0 {
            Some(Self(q))
        } else {
            None
        }
    }
    pub(crate) fn at_least_ratio(self, denominator: Self, numerator: i128, divisor: i128) -> bool {
        if self.0 < 0 || denominator.0 <= 0 || numerator < 0 || divisor <= 0 {
            return false;
        }
        let scaled = denominator.0 / divisor * numerator;
        let remainder = (denominator.0 % divisor) * numerator;
        let threshold = scaled.saturating_add((remainder + divisor - 1) / divisor);
        self.0 >= threshold
    }
    pub(crate) fn ratio_is_exact(num: Self, den: Self) -> bool {
        if den.0 == 0 {
            return false;
        }
        let n = num.0.unsigned_abs();
        let d = den.0.unsigned_abs();
        let Some(scaled) = n.checked_mul(100) else {
            return false;
        };
        let mut remainder = scaled % d;
        for _ in 0..18 {
            let Some(next) = remainder.checked_mul(10) else {
                return false;
            };
            remainder = next % d;
        }
        remainder == 0
    }
    pub(crate) fn rounded_div(self, d: i128) -> Option<Self> {
        if d == 0 {
            return None;
        }
        let q = self.0 / d;
        let r = self.0 % d;
        let twice = r.unsigned_abs().checked_mul(2)?;
        Some(Self(if twice >= d.unsigned_abs() {
            q.checked_add(if (self.0 < 0) ^ (d < 0) { -1 } else { 1 })?
        } else {
            q
        }))
    }
    /// Rounded half away from zero to fixed scale; denominator must be positive.
    pub(crate) fn ratio_percent(num: Self, den: Self) -> Option<Self> {
        if den.0 == 0 {
            return None;
        }
        // Compute 100*num/den in scale units by decimal long division; each
        // remainder is below |den| <= 1e33, so multiplying it by ten is safe.
        let negative = (num.0 < 0) ^ (den.0 < 0);
        let n = num.0.unsigned_abs();
        let d = den.0.unsigned_abs();
        let scaled = n.checked_mul(100)?;
        let mut whole = scaled / d;
        let mut rem = scaled % d;
        let mut frac = 0u128;
        for _ in 0..18 {
            rem = rem.checked_mul(10)?;
            frac = frac.checked_mul(10)?.checked_add(rem / d)?;
            rem %= d;
        }
        if rem.checked_mul(2)? >= d {
            frac = frac.checked_add(1)?;
            if frac >= Self::SCALE as u128 {
                frac = 0;
                whole = whole.checked_add(1)?;
            }
        }
        let value = whole.checked_mul(Self::SCALE as u128)?.checked_add(frac)?;
        let signed = i128::try_from(value).ok()?;
        if value > Self::MAX as u128 {
            return None;
        }
        Some(Self(if negative { -signed } else { signed }))
    }
}
impl fmt::Display for Money {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let neg = self.0 < 0;
        let n = self.0.unsigned_abs();
        let w = n / (Self::SCALE as u128);
        let frac = n % (Self::SCALE as u128);
        if frac == 0 {
            return write!(f, "{}{}", if neg { "-" } else { "" }, w);
        }
        let s = format!("{:018}", frac);
        write!(
            f,
            "{}{}.{}",
            if neg { "-" } else { "" },
            w,
            s.trim_end_matches('0')
        )
    }
}
impl FromStr for Money {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        Self::parse(s)
    }
}
impl Serialize for Money {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}
impl<'de> Deserialize<'de> for Money {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Self::parse(&s).map_err(de::Error::custom)
    }
}
impl Add for Money {
    type Output = Self;
    fn add(self, r: Self) -> Self {
        self.checked_add(r).expect("bounded money sum")
    }
}
impl AddAssign for Money {
    fn add_assign(&mut self, r: Self) {
        *self = *self + r
    }
}
impl Sub for Money {
    type Output = Self;
    fn sub(self, r: Self) -> Self {
        self + (-r)
    }
}
impl SubAssign for Money {
    fn sub_assign(&mut self, r: Self) {
        *self = *self - r
    }
}
impl Neg for Money {
    type Output = Self;
    fn neg(self) -> Self {
        Self(self.0.checked_neg().expect("bounded money negation"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_decimal_and_serde() {
        let a = Money::parse("0.1").unwrap();
        assert_eq!((a + a + a).to_string(), "0.3");
        assert_eq!(
            Money::parse("-0.000000000000000001").unwrap().to_string(),
            "-0.000000000000000001"
        );
        assert_eq!(serde_json::to_string(&a).unwrap(), "\"0.1\"");
        assert_eq!(serde_json::from_str::<Money>("\"0.1\"").unwrap(), a);
        assert_eq!(
            Money::parse("1.0000000000000000001"),
            Err("decimal precision exceeds 18 places".into())
        );
        assert!(Money::parse("1000000000000001").is_err());
        assert_eq!(
            Money::parse("1").unwrap().checked_div(2),
            Some(Money::parse("0.5").unwrap())
        );
        assert_eq!(Money::parse("1").unwrap().checked_div(3), None);
        assert_eq!(
            Money::parse("3").unwrap().checked_div(2),
            Some(Money::parse("1.5").unwrap())
        );
    }

    #[test]
    fn ratio_exactness_matches_eighteen_decimal_places() {
        assert!(Money::ratio_is_exact(
            Money::parse("1").unwrap(),
            Money::parse("8").unwrap()
        ));
        assert!(!Money::ratio_is_exact(
            Money::parse("1").unwrap(),
            Money::parse("3").unwrap()
        ));
        assert!(Money::ratio_is_exact(
            Money::parse("0.000000000000000001").unwrap(),
            Money::parse("1").unwrap()
        ));
    }
}
