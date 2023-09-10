use serde::ser::SerializeStructVariant;

const TIME_FMT: &str = "%H:%M:%S";

#[derive(Debug)]
pub enum ChargeStatus {
    Discharging {
        time_remain: Option<chrono::NaiveTime>,
    },
    Charging {
        time_remain: Option<chrono::NaiveTime>,
    },
    NotCharging,
}

impl serde::Serialize for ChargeStatus {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        #[allow(unused_assignments)]
        let mut time_string: String = "".to_string();

        match *self {
            ChargeStatus::Discharging { time_remain } => {
                let mut state =
                    serializer.serialize_struct_variant("ChargeStatus", 0, "Discharging", 1)?;
                state.serialize_field(
                    "time_remain",
                    match time_remain {
                        Some(time) => {
                            time_string = time.to_string();
                            &time_string
                        }
                        None => {
                            time_string = "none".to_string();
                            &time_string
                        }
                    },
                )?;
                state.end()
            }
            ChargeStatus::Charging { time_remain } => {
                let mut state =
                    serializer.serialize_struct_variant("ChargeStatus", 1, "Charging", 1)?;

                state.serialize_field(
                    "time_remain",
                    match time_remain {
                        Some(time) => {
                            time_string = time.to_string();
                            &time_string
                        }
                        None => {
                            time_string = "none".to_string();
                            &time_string
                        }
                    },
                )?;
                state.end()
            }
            ChargeStatus::NotCharging => {
                let state =
                    serializer.serialize_struct_variant("ChargeStatus", 2, "NotCharging", 0)?;
                state.end()
            }
        }
    }
}

struct ChargeStatusVisitor {}

impl<'de> serde::de::Visitor<'de> for ChargeStatusVisitor {
    type Value = ChargeStatus;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("Could not deserialize data!")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        let mut status = ChargeStatus::NotCharging;

        while let Some((key, value)) = map.next_entry::<String, RemainTime>()? {
            match key.as_str() {
                "Charging" => {
                    status = ChargeStatus::Charging {
                        time_remain: value.time_remain,
                    }
                }
                "Discharging" => {
                    status = ChargeStatus::Discharging {
                        time_remain: value.time_remain,
                    }
                }
                _ => {}
            }
        }

        Ok(status)
    }
}

impl<'de> serde::Deserialize<'de> for ChargeStatus {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_map(ChargeStatusVisitor {})
    }
}

struct RemainTime {
    time_remain: Option<chrono::NaiveTime>,
}

struct RemainTimeVisitor {}

impl<'de> serde::de::Visitor<'de> for RemainTimeVisitor {
    type Value = RemainTime;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("Could not deserialize data!")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        let mut out = RemainTime { time_remain: None };

        while let Some((key, value)) = map.next_entry::<String, String>()? {
            match key.as_str() {
                "time_remain" => {
                    out.time_remain =
                        Some(chrono::NaiveTime::parse_from_str(&value, TIME_FMT).unwrap());
                }
                _ => {}
            }
        }

        Ok(out)
    }
}

impl<'de> serde::Deserialize<'de> for RemainTime {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_map(RemainTimeVisitor {})
    }
}

impl PartialEq for ChargeStatus {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Discharging { time_remain: _ }, Self::Discharging { time_remain: _ }) => true,
            (Self::Charging { time_remain: _ }, Self::Charging { time_remain: _ }) => true,
            _ => core::mem::discriminant(self) == core::mem::discriminant(other),
        }
    }
}
