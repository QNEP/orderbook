use std::{convert::TryFrom, fmt::Display};

use crate::lookup_tables::DECIMAL_SHRINK_MULTIPLIERS_F64;

use super::lookup_tables::MAX_DECIMALS;

/// Error when creating Decimals from out-of-range values
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecimalRangeError;

impl Display for DecimalRangeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "invalid decimals, range must be between 0 and {}",
            MAX_DECIMALS
        )
    }
}

/// Represents a decimal places value constrained to 0-18
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Decimals(u8);

impl Decimals {
    pub fn new<N: TryInto<u8>>(value: N) -> Result<Decimals, DecimalRangeError> {
        let value = value.try_into().map_err(|_| DecimalRangeError)?;
        if value <= MAX_DECIMALS {
            Ok(Self(value))
        } else {
            Err(DecimalRangeError)
        }
    }

    pub fn value(&self) -> u8 {
        self.0
    }

    #[inline(always)]
    fn shrink_multiplier_f64(&self) -> f64 {
        // SAFETY new validates self.0 is in range
        unsafe { *DECIMAL_SHRINK_MULTIPLIERS_F64.get_unchecked(self.0 as usize) }
    }

    #[inline]
    pub fn reference_tick_to_f64(&self, tick: u32) -> f64 {
        let f = tick as f64;
        f * 10.0f64.powi(-(self.0 as i32))
    }

    #[inline]
    pub fn fast_tick_to_f64(&self, tick: u32) -> f64 {
        (tick as f64) * self.shrink_multiplier_f64()
    }
}

impl TryFrom<u8> for Decimals {
    type Error = DecimalRangeError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Decimals::new(value)
    }
}

impl TryFrom<u16> for Decimals {
    type Error = DecimalRangeError;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        Decimals::new(value)
    }
}

impl TryFrom<u32> for Decimals {
    type Error = DecimalRangeError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        Decimals::new(value)
    }
}

impl TryFrom<u64> for Decimals {
    type Error = DecimalRangeError;

    fn try_from(value: u64) -> Result<Self, Self::Error> {
        Decimals::new(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tick_to_f64() {
        let tick = u32::MAX;
        let decimals = Decimals::new(3u8).unwrap();

        let reference_result = decimals.reference_tick_to_f64(tick);
        let fast_result = decimals.fast_tick_to_f64(tick);

        assert_eq!(reference_result, fast_result);
        println!("Reference: {}, Fast: {}", reference_result, fast_result);
    }

    #[test]
    fn compare_tick_conversion_methods_f64() {
        let tick = u32::MAX;
        for decimals in 0..=MAX_DECIMALS {
            let decimals = Decimals::new(decimals).unwrap();

            let reference = decimals.reference_tick_to_f64(tick);
            let fast = decimals.fast_tick_to_f64(tick);
            assert_eq!(reference, fast);
        }
    }
}
