//! Pure refund domain: types, policy rules, the deterministic engine, the prose
//! renderer and the heuristic pre-scan. No I/O and no async live here, so every
//! decision the system makes can be tested without a database or an LLM.

/// A fieldless enum whose string form is used both by serde and by the database
/// CHECK constraint on the column that stores it. Each variant names its string
/// once, so the two can never drift apart.
macro_rules! string_enum {
    ($(#[$meta:meta])* $vis:vis enum $name:ident { $($variant:ident = $s:literal),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
        $vis enum $name {
            $(#[serde(rename = $s)] $variant),+
        }

        impl $name {
            pub const ALL: &'static [$name] = &[$($name::$variant),+];

            pub fn as_str(self) -> &'static str {
                match self {
                    $($name::$variant => $s),+
                }
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl std::str::FromStr for $name {
            type Err = $crate::types::UnknownVariant;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                match s {
                    $($s => Ok($name::$variant),)+
                    _ => Err($crate::types::UnknownVariant {
                        type_name: stringify!($name),
                        value: s.to_owned(),
                    }),
                }
            }
        }
    };
}

pub mod engine;
pub mod money;
pub mod policy;
pub mod prescan;
pub mod prose;
pub mod types;
