//! What a unit measures, and how one unit relates to another measuring the same
//! thing.
//!
//! A unit is written as an ordinary word, so any word is a unit and the
//! vocabulary stays open. A word this table knows also carries a dimension: what
//! it measures, and how far its scale sits from the canonical unit for that
//! dimension. That is what lets `10 g/L * 500 mL` be `5 g` rather than a
//! quantity in neither operand's unit, and what lets `12 kb` be compared with a
//! length in base pairs.
//!
//! A word the table does not know measures something this compiler has no
//! opinion about. It can still be written, held in a field, and compared with
//! itself; it cannot be converted or composed, because nothing here knows what
//! it would convert to.

use std::fmt;

/// What a measurement measures, as powers of the things a laboratory counts.
///
/// Volume is its own base rather than a cube of length. A laboratory measures
/// in litres, not in cubic metres, and deriving one from the other would make
/// every volume carry an exponent nobody wrote.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct Dimension {
    mass: i8,
    volume: i8,
    amount: i8,
    length: i8,
    duration: i8,
    temperature: i8,
    /// Things counted rather than measured: colonies, base pairs, cycles.
    count: i8,
}

impl Dimension {
    const DIMENSIONLESS: Self = Self::of([0, 0, 0, 0, 0, 0, 0]);
    const MASS: Self = Self::of([1, 0, 0, 0, 0, 0, 0]);
    const VOLUME: Self = Self::of([0, 1, 0, 0, 0, 0, 0]);
    const AMOUNT: Self = Self::of([0, 0, 1, 0, 0, 0, 0]);
    const LENGTH: Self = Self::of([0, 0, 0, 1, 0, 0, 0]);
    const DURATION: Self = Self::of([0, 0, 0, 0, 1, 0, 0]);
    const TEMPERATURE: Self = Self::of([0, 0, 0, 0, 0, 1, 0]);
    const COUNT: Self = Self::of([0, 0, 0, 0, 0, 0, 1]);
    /// Mass in a volume, which is what a medium recipe is written in.
    const CONCENTRATION: Self = Self::of([1, -1, 0, 0, 0, 0, 0]);
    /// Amount in a volume, which is what a buffer is written in.
    const MOLARITY: Self = Self::of([0, -1, 1, 0, 0, 0, 0]);

    const fn of(powers: [i8; 7]) -> Self {
        Self {
            mass: powers[0],
            volume: powers[1],
            amount: powers[2],
            length: powers[3],
            duration: powers[4],
            temperature: powers[5],
            count: powers[6],
        }
    }

    fn combined(self, other: Self, sign: i8) -> Self {
        Self {
            mass: self.mass + sign * other.mass,
            volume: self.volume + sign * other.volume,
            amount: self.amount + sign * other.amount,
            length: self.length + sign * other.length,
            duration: self.duration + sign * other.duration,
            temperature: self.temperature + sign * other.temperature,
            count: self.count + sign * other.count,
        }
    }

    pub(crate) fn times(self, other: Self) -> Self {
        self.combined(other, 1)
    }

    pub(crate) fn over(self, other: Self) -> Self {
        self.combined(other, -1)
    }

    pub(crate) fn is_dimensionless(self) -> bool {
        self == Self::DIMENSIONLESS
    }

    /// The name this dimension is written as where a field asks for one.
    ///
    /// Only the dimensions a person names have one. A derived dimension such as
    /// mass over volume is a real answer for arithmetic to produce and not
    /// something a schema asks for, so it has no name and is described by the
    /// unit it came out in.
    pub(crate) fn name(self) -> Option<&'static str> {
        Some(match self {
            Self::MASS => "Mass",
            Self::VOLUME => "Volume",
            Self::AMOUNT => "Amount",
            Self::LENGTH => "Length",
            Self::DURATION => "Duration",
            Self::TEMPERATURE => "Temperature",
            Self::COUNT => "Count",
            Self::CONCENTRATION => "Concentration",
            Self::MOLARITY => "Molarity",
            _ => return None,
        })
    }

    /// This dimension split into what it is and what it is per, when it is
    /// exactly one of each. Mass over volume splits; mass over volume squared
    /// does not.
    fn as_ratio(self) -> Option<(Self, Self)> {
        let mut numerator = [0i8; 7];
        let mut denominator = [0i8; 7];
        for (index, power) in self.powers().into_iter().enumerate() {
            match power {
                0 => {}
                1 => numerator[index] = 1,
                -1 => denominator[index] = 1,
                _ => return None,
            }
        }
        let (numerator, denominator) = (Self::of(numerator), Self::of(denominator));
        (!numerator.is_dimensionless() && !denominator.is_dimensionless())
            .then_some((numerator, denominator))
    }

    fn powers(self) -> [i8; 7] {
        [
            self.mass,
            self.volume,
            self.amount,
            self.length,
            self.duration,
            self.temperature,
            self.count,
        ]
    }

    /// The dimension a field names, if that word names one.
    pub(crate) fn named(name: &str) -> Option<Self> {
        Some(match name {
            "Mass" => Self::MASS,
            "Volume" => Self::VOLUME,
            "Amount" => Self::AMOUNT,
            "Length" => Self::LENGTH,
            "Duration" => Self::DURATION,
            "Temperature" => Self::TEMPERATURE,
            "Count" => Self::COUNT,
            "Concentration" => Self::CONCENTRATION,
            "Molarity" => Self::MOLARITY,
            _ => return None,
        })
    }
}

/// What one unit measures and where its scale sits.
///
/// `decade` is the power of ten that takes this unit to the canonical unit for
/// its dimension: a nanogram is `-9` because a nanogram is `10^-9` grams. Powers
/// of ten keep every conversion exact, which decimal magnitudes then carry
/// without rounding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Measured {
    pub dimension: Dimension,
    pub decade: i32,
}

impl Measured {
    const fn new(dimension: Dimension, decade: i32) -> Self {
        Self { dimension, decade }
    }
}

/// Units whose meaning this compiler knows, with the canonical unit of each
/// dimension at decade zero.
///
/// Celsius is deliberately absent. Converting a temperature is an offset rather
/// than a scale, so a table of powers of ten would get it wrong, and a
/// laboratory writes degrees Celsius and means them.
const TABLE: &[(&str, Measured)] = &[
    ("g", Measured::new(Dimension::MASS, 0)),
    ("kg", Measured::new(Dimension::MASS, 3)),
    ("mg", Measured::new(Dimension::MASS, -3)),
    ("ug", Measured::new(Dimension::MASS, -6)),
    ("ng", Measured::new(Dimension::MASS, -9)),
    ("pg", Measured::new(Dimension::MASS, -12)),
    ("L", Measured::new(Dimension::VOLUME, 0)),
    ("mL", Measured::new(Dimension::VOLUME, -3)),
    ("uL", Measured::new(Dimension::VOLUME, -6)),
    ("nL", Measured::new(Dimension::VOLUME, -9)),
    ("pL", Measured::new(Dimension::VOLUME, -12)),
    ("mol", Measured::new(Dimension::AMOUNT, 0)),
    ("mmol", Measured::new(Dimension::AMOUNT, -3)),
    ("umol", Measured::new(Dimension::AMOUNT, -6)),
    ("nmol", Measured::new(Dimension::AMOUNT, -9)),
    ("pmol", Measured::new(Dimension::AMOUNT, -12)),
    ("fmol", Measured::new(Dimension::AMOUNT, -15)),
    ("m", Measured::new(Dimension::LENGTH, 0)),
    ("mm", Measured::new(Dimension::LENGTH, -3)),
    ("um", Measured::new(Dimension::LENGTH, -6)),
    ("nm", Measured::new(Dimension::LENGTH, -9)),
    ("s", Measured::new(Dimension::DURATION, 0)),
    ("min", Measured::new(Dimension::DURATION, 0)),
    ("h", Measured::new(Dimension::DURATION, 0)),
    ("d", Measured::new(Dimension::DURATION, 0)),
    ("M", Measured::new(Dimension::MOLARITY, 0)),
    ("mM", Measured::new(Dimension::MOLARITY, -3)),
    ("uM", Measured::new(Dimension::MOLARITY, -6)),
    ("nM", Measured::new(Dimension::MOLARITY, -9)),
    ("bp", Measured::new(Dimension::COUNT, 0)),
    ("kb", Measured::new(Dimension::COUNT, 3)),
    ("Mb", Measured::new(Dimension::COUNT, 6)),
    ("cfu", Measured::new(Dimension::COUNT, 0)),
];

/// The units of one dimension whose scales are not powers of ten.
///
/// An hour is 3600 seconds, not `10^n` seconds, so duration cannot ride the
/// decade table. These convert against each other exactly and against nothing
/// else.
const DURATIONS: &[(&str, u64)] = &[("s", 1), ("min", 60), ("h", 3_600), ("d", 86_400)];

/// What a unit measures, reading a compound unit as its numerator over its
/// denominator.
pub(crate) fn measured(unit: &str) -> Option<Measured> {
    if let Some((numerator, denominator)) = unit.split_once('/') {
        let numerator = simple(numerator)?;
        let denominator = simple(denominator)?;
        return Some(Measured {
            dimension: numerator.dimension.over(denominator.dimension),
            decade: numerator.decade - denominator.decade,
        });
    }
    simple(unit)
}

fn simple(unit: &str) -> Option<Measured> {
    TABLE
        .iter()
        .find(|(name, _)| *name == unit)
        .map(|(_, measured)| *measured)
}

/// How many of `to` one `from` is, when both measure the same thing.
///
/// The answer is a ratio of whole numbers so a magnitude converts exactly. A
/// unit this table does not know, or two units measuring different things, have
/// no ratio and convert to nothing.
pub(crate) fn ratio(from: &str, to: &str) -> Option<(u64, u64)> {
    if from == to {
        return Some((1, 1));
    }
    let (source, target) = (measured(from)?, measured(to)?);
    if source.dimension != target.dimension {
        return None;
    }
    // Durations scale by sixties rather than by tens.
    if source.dimension == Dimension::DURATION {
        let seconds = |unit: &str| {
            DURATIONS
                .iter()
                .find(|(name, _)| *name == unit)
                .map(|it| it.1)
        };
        return Some((seconds(from)?, seconds(to)?));
    }
    let decades = source.decade - target.decade;
    let power = 10u64.checked_pow(decades.unsigned_abs())?;
    Some(if decades >= 0 { (power, 1) } else { (1, power) })
}

/// The unit a derived measurement is expressed in.
///
/// A product or a quotient lands in the canonical unit of whatever it came out
/// measuring, so the result of `10 g/L * 500 mL` is grams and not some scaled
/// unit nobody wrote. Where the dimension has no canonical spelling, the
/// arithmetic has no unit to report.
pub(crate) fn canonical(dimension: Dimension) -> Option<String> {
    if dimension.is_dimensionless() {
        return None;
    }
    if let Some(unit) = base_unit(dimension) {
        return Some(unit.to_owned());
    }
    // A derived dimension is written as the canonical unit of what it is over
    // the canonical unit of what it is per, which is how mass over volume comes
    // back out as `g/L`. Anything more layered than that has no spelling here,
    // and saying so beats inventing one.
    let (numerator, denominator) = dimension.as_ratio()?;
    Some(format!(
        "{}/{}",
        base_unit(numerator)?,
        base_unit(denominator)?
    ))
}

/// The canonical unit of a dimension that is one base thing, unscaled.
fn base_unit(dimension: Dimension) -> Option<&'static str> {
    TABLE
        .iter()
        .find(|(_, measured)| measured.dimension == dimension && measured.decade == 0)
        .map(|(name, _)| *name)
}

impl fmt::Display for Dimension {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.name() {
            Some(name) => formatter.write_str(name),
            None => formatter.write_str("a derived measurement"),
        }
    }
}

/// An exact decimal: `digits * 10^exponent`.
///
/// Quantities compose by multiplying magnitudes and adding the powers of ten
/// their units sit at, and both stay exact when the magnitude never becomes a
/// float. A recipe scaled to a batch is weighed out on a balance, so a rounding
/// here is a rounding there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Decimal {
    digits: i128,
    exponent: i32,
}

impl Decimal {
    /// Read a plain decimal, which is the only spelling that reaches here: an
    /// exponent literal was written out where it was lexed.
    pub(crate) fn parse(text: &str) -> Option<Self> {
        let (sign, rest) = match text.strip_prefix('-') {
            Some(rest) => (-1, rest),
            None => (1, text.strip_prefix('+').unwrap_or(text)),
        };
        let (whole, fraction) = rest.split_once('.').unwrap_or((rest, ""));
        if whole.is_empty() && fraction.is_empty() {
            return None;
        }
        if !whole
            .chars()
            .chain(fraction.chars())
            .all(|c| c.is_ascii_digit())
        {
            return None;
        }
        let digits: i128 = format!("{whole}{fraction}").parse().ok()?;
        Some(Self {
            digits: sign * digits,
            exponent: -(fraction.len() as i32),
        })
    }

    pub(crate) fn times(self, other: Self) -> Option<Self> {
        Some(Self {
            digits: self.digits.checked_mul(other.digits)?,
            exponent: self.exponent.checked_add(other.exponent)?,
        })
    }

    /// Divide, when the quotient terminates.
    ///
    /// A third of a gram has no exact decimal, and rounding one silently is how
    /// a balance ends up reading something nobody wrote. Refusing is the honest
    /// answer.
    pub(crate) fn over(self, other: Self) -> Option<Self> {
        if other.digits == 0 {
            return None;
        }
        // Lengthen the numerator until the division comes out whole, within the
        // range a decimal magnitude can hold.
        let mut digits = self.digits;
        let mut exponent = self.exponent;
        for _ in 0..38 {
            if digits % other.digits == 0 {
                return Some(Self {
                    digits: digits / other.digits,
                    exponent: exponent.checked_sub(other.exponent)?,
                });
            }
            digits = digits.checked_mul(10)?;
            exponent = exponent.checked_sub(1)?;
        }
        None
    }

    pub(crate) fn shifted(self, decades: i32) -> Option<Self> {
        Some(Self {
            digits: self.digits,
            exponent: self.exponent.checked_add(decades)?,
        })
    }

    /// Multiply by a whole ratio, which is how a duration converts.
    pub(crate) fn scaled(self, numerator: u64, denominator: u64) -> Option<Self> {
        let scaled = Self {
            digits: self.digits.checked_mul(i128::from(numerator))?,
            exponent: self.exponent,
        };
        scaled.over(Self {
            digits: i128::from(denominator),
            exponent: 0,
        })
    }
}

impl fmt::Display for Decimal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.exponent >= 0 {
            return write!(
                formatter,
                "{}{}",
                self.digits,
                "0".repeat(self.exponent as usize)
            );
        }
        let places = self.exponent.unsigned_abs() as usize;
        let sign = if self.digits < 0 { "-" } else { "" };
        let digits = self.digits.unsigned_abs().to_string();
        let digits = if digits.len() <= places {
            format!("{}{digits}", "0".repeat(places - digits.len() + 1))
        } else {
            digits
        };
        let point = digits.len() - places;
        let fraction = digits[point..].trim_end_matches('0');
        if fraction.is_empty() {
            return write!(formatter, "{sign}{}", &digits[..point]);
        }
        write!(formatter, "{sign}{}.{fraction}", &digits[..point])
    }
}
