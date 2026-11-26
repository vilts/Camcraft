//! Provenance: Autodesk DXF HEADER $INSUNITS codes and standard length conversions.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Units {
    Known(Unit),
    #[default]
    MissingOrUnitless,
    Unsupported(i32),
}

/// DXF drawing units, separate from the application's two display units.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unit {
    Inches,
    Feet,
    Miles,
    Millimeters,
    Centimeters,
    Meters,
    Kilometers,
    Microinches,
    Mils,
    Yards,
    Angstroms,
    Nanometers,
    Microns,
    Decimeters,
    Decameters,
    Hectometers,
    Gigameters,
    AstronomicalUnits,
    LightYears,
    Parsecs,
    UsSurveyFeet,
    UsSurveyInches,
    UsSurveyYards,
    UsSurveyMiles,
}

impl Units {
    /// The `$INSUNITS` code this value reads back as: 0 for missing or unitless.
    pub fn code(self) -> i32 {
        match self {
            Self::Known(unit) => unit.code(),
            Self::MissingOrUnitless => 0,
            Self::Unsupported(code) => code,
        }
    }

    /// The value of a `$INSUNITS` code (the DXF reference's table).
    pub fn from_code(code: i32) -> Self {
        let unit = match code {
            0 => return Self::MissingOrUnitless,
            1 => Unit::Inches,
            2 => Unit::Feet,
            3 => Unit::Miles,
            4 => Unit::Millimeters,
            5 => Unit::Centimeters,
            6 => Unit::Meters,
            7 => Unit::Kilometers,
            8 => Unit::Microinches,
            9 => Unit::Mils,
            10 => Unit::Yards,
            11 => Unit::Angstroms,
            12 => Unit::Nanometers,
            13 => Unit::Microns,
            14 => Unit::Decimeters,
            15 => Unit::Decameters,
            16 => Unit::Hectometers,
            17 => Unit::Gigameters,
            18 => Unit::AstronomicalUnits,
            19 => Unit::LightYears,
            20 => Unit::Parsecs,
            21 => Unit::UsSurveyFeet,
            22 => Unit::UsSurveyInches,
            23 => Unit::UsSurveyYards,
            24 => Unit::UsSurveyMiles,
            _ => return Self::Unsupported(code),
        };
        Self::Known(unit)
    }
}

impl Unit {
    /// The Autodesk `$INSUNITS` group 70 code for this unit (#744).
    #[must_use]
    pub fn insunits_code(self) -> i32 {
        match self {
            Self::Inches => 1,
            Self::Feet => 2,
            Self::Miles => 3,
            Self::Millimeters => 4,
            Self::Centimeters => 5,
            Self::Meters => 6,
            Self::Kilometers => 7,
            Self::Microinches => 8,
            Self::Mils => 9,
            Self::Yards => 10,
            Self::Angstroms => 11,
            Self::Nanometers => 12,
            Self::Microns => 13,
            Self::Decimeters => 14,
            Self::Decameters => 15,
            Self::Hectometers => 16,
            Self::Gigameters => 17,
            Self::AstronomicalUnits => 18,
            Self::LightYears => 19,
            Self::Parsecs => 20,
            Self::UsSurveyFeet => 21,
            Self::UsSurveyInches => 22,
            Self::UsSurveyYards => 23,
            Self::UsSurveyMiles => 24,
        }
    }

    /// Metadata only. Scaling geometry is a caller decision and needs another range check.
    pub fn millimeters_per_unit(self) -> f64 {
        match self {
            Self::Inches => 25.4,
            Self::Feet => 304.8,
            Self::Miles => 1_609_344.0,
            Self::Millimeters => 1.0,
            Self::Centimeters => 10.0,
            Self::Meters => 1000.0,
            Self::Kilometers => 1_000_000.0,
            Self::Microinches => 0.0000254,
            Self::Mils => 0.0254,
            Self::Yards => 914.4,
            Self::Angstroms => 1e-7,
            Self::Nanometers => 1e-6,
            Self::Microns => 0.001,
            Self::Decimeters => 100.0,
            Self::Decameters => 10_000.0,
            Self::Hectometers => 100_000.0,
            Self::Gigameters => 1e12,
            Self::AstronomicalUnits => 149_597_870_700_000.0,
            Self::LightYears => 9_460_730_472_580_800_000.0,
            Self::Parsecs => 149_597_870_700_000.0 * 648_000.0 / std::f64::consts::PI,
            Self::UsSurveyFeet => 1_200_000.0 / 3937.0,
            Self::UsSurveyInches => 100_000.0 / 3937.0,
            Self::UsSurveyYards => 3_600_000.0 / 3937.0,
            Self::UsSurveyMiles => 6_336_000_000.0 / 3937.0,
        }
    }

    /// Autodesk DXF `$INSUNITS` group 70 integer code.
    #[must_use]
    pub const fn code(self) -> i32 {
        match self {
            Self::Inches => 1,
            Self::Feet => 2,
            Self::Miles => 3,
            Self::Millimeters => 4,
            Self::Centimeters => 5,
            Self::Meters => 6,
            Self::Kilometers => 7,
            Self::Microinches => 8,
            Self::Mils => 9,
            Self::Yards => 10,
            Self::Angstroms => 11,
            Self::Nanometers => 12,
            Self::Microns => 13,
            Self::Decimeters => 14,
            Self::Decameters => 15,
            Self::Hectometers => 16,
            Self::Gigameters => 17,
            Self::AstronomicalUnits => 18,
            Self::LightYears => 19,
            Self::Parsecs => 20,
            Self::UsSurveyFeet => 21,
            Self::UsSurveyInches => 22,
            Self::UsSurveyYards => 23,
            Self::UsSurveyMiles => 24,
        }
    }
}
