//! Which Java a Minecraft version needs, and which installed Java fits best.
//!
//! Pure logic: probing executables and downloading runtimes happen in the
//! application.

/// Major version assumed when version metadata does not declare one. Matches
/// Mojang's convention for legacy versions.
pub const DEFAULT_JAVA_MAJOR_VERSION: u32 = 8;

/// Main class of the legacy mod loader that only works on Java 8.
const LAUNCHWRAPPER_MAIN_CLASS: &str = "net.minecraft.launchwrapper.Launch";

/// Which Java a Minecraft version needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JavaRequirement {
    /// Minimum supported major version.
    pub minimum_major_version: u32,
    /// Inclusive upper bound for legacy main classes that break on newer JVMs.
    pub maximum_major_version: Option<u32>,
}

impl JavaRequirement {
    /// Reads the requirement from version metadata: `javaVersion.majorVersion`
    /// and `mainClass`.
    pub fn from_metadata(major_version: Option<i64>, main_class: Option<&str>) -> Self {
        let minimum_major_version = major_version
            .and_then(|major| u32::try_from(major).ok())
            .unwrap_or(DEFAULT_JAVA_MAJOR_VERSION);
        let maximum_major_version =
            (main_class == Some(LAUNCHWRAPPER_MAIN_CLASS)).then_some(DEFAULT_JAVA_MAJOR_VERSION);
        Self {
            minimum_major_version,
            maximum_major_version,
        }
    }

    /// Human-readable label, such as `Java 21+` or `Java 8`.
    pub fn label(&self) -> String {
        match self.maximum_major_version {
            Some(maximum) => format!("Java {maximum}"),
            None => format!("Java {}+", self.minimum_major_version),
        }
    }

    /// Whether a runtime with this major version can run the version.
    pub fn is_satisfied_by(&self, major_version: u32) -> bool {
        major_version >= self.minimum_major_version
            && self
                .maximum_major_version
                .is_none_or(|maximum| major_version <= maximum)
    }
}

/// Extracts the major version from a `java -version` string such as `21.0.8`
/// or the legacy `1.8.0_412`.
pub fn parse_major_version(version: &str) -> Option<u32> {
    let mut components = version
        .split(|character: char| !character.is_ascii_digit())
        .filter(|component| !component.is_empty())
        .filter_map(|component| component.parse::<u32>().ok());
    let first = components.next()?;
    if first == 1 {
        components.next()
    } else {
        Some(first)
    }
}

/// How strictly an installed runtime must match the requirement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeMatch {
    /// Exactly the major version the game declares. Older modded versions
    /// break on newer Java, so this is what launches use first; when nothing
    /// matches, the declared runtime is downloaded.
    Exact,
    /// Any runtime the requirement allows; the fallback when the declared
    /// runtime cannot be downloaded.
    Compatible,
}

/// Picks the runtime to use among installed ones. `describe` returns the
/// major version and the full version string of a runtime.
///
/// The closest major version wins (exact first, then the nearest newer);
/// among equals the highest version string wins.
pub fn pick_runtime<'a, T>(
    runtimes: &'a [T],
    requirement: &JavaRequirement,
    matching: RuntimeMatch,
    describe: impl Fn(&T) -> (u32, &str),
) -> Option<&'a T> {
    runtimes
        .iter()
        .filter(|runtime| {
            let major_version = describe(runtime).0;
            match matching {
                RuntimeMatch::Exact => major_version == requirement.minimum_major_version,
                RuntimeMatch::Compatible => requirement.is_satisfied_by(major_version),
            }
        })
        .min_by_key(|runtime| {
            let (major_version, version) = describe(runtime);
            (
                major_version.abs_diff(requirement.minimum_major_version),
                std::cmp::Reverse(version),
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pick<'a>(runtimes: &'a [(u32, &'a str)], requirement: JavaRequirement) -> Option<&'a str> {
        pick_runtime(
            runtimes,
            &requirement,
            RuntimeMatch::Compatible,
            |runtime| (runtime.0, runtime.1),
        )
        .map(|runtime| runtime.1)
    }

    #[test]
    fn exact_match_ignores_newer_runtimes() {
        let runtimes = [(21, "21.0.8"), (25, "25.0.1")];
        let exact = |requirement: JavaRequirement| {
            pick_runtime(&runtimes, &requirement, RuntimeMatch::Exact, |runtime| {
                (runtime.0, runtime.1)
            })
            .map(|runtime| runtime.1)
        };
        // 1.16.5 declares Java 8: a newer Java must not be used silently.
        assert_eq!(exact(at_least(8)), None);
        assert_eq!(exact(at_least(21)), Some("21.0.8"));
    }

    fn at_least(major: u32) -> JavaRequirement {
        JavaRequirement {
            minimum_major_version: major,
            maximum_major_version: None,
        }
    }

    #[test]
    fn modern_version_requires_metadata_java() {
        let requirement = JavaRequirement::from_metadata(Some(25), None);
        assert_eq!(requirement.minimum_major_version, 25);
        assert_eq!(requirement.label(), "Java 25+");
        assert!(requirement.is_satisfied_by(25));
        assert!(requirement.is_satisfied_by(26));
        assert!(!requirement.is_satisfied_by(21));
    }

    #[test]
    fn legacy_version_defaults_to_java_8() {
        let requirement = JavaRequirement::from_metadata(None, None);
        assert_eq!(
            requirement.minimum_major_version,
            DEFAULT_JAVA_MAJOR_VERSION
        );
        assert!(requirement.is_satisfied_by(21));
    }

    #[test]
    fn launchwrapper_is_capped_at_java_8() {
        let requirement = JavaRequirement::from_metadata(None, Some(LAUNCHWRAPPER_MAIN_CLASS));
        assert_eq!(requirement.label(), "Java 8");
        assert!(requirement.is_satisfied_by(8));
        assert!(!requirement.is_satisfied_by(9));
        assert!(!requirement.is_satisfied_by(21));
    }

    #[test]
    fn parse_major_version_handles_modern_and_legacy() {
        assert_eq!(parse_major_version("21.0.8"), Some(21));
        assert_eq!(parse_major_version("25"), Some(25));
        assert_eq!(parse_major_version("1.8.0_412"), Some(8));
        assert_eq!(parse_major_version(""), None);
    }

    #[test]
    fn prefers_exact_major() {
        let runtimes = [(17, "17.0.2"), (21, "21.0.8"), (25, "25.0.1")];
        assert_eq!(pick(&runtimes, at_least(25)), Some("25.0.1"));
    }

    #[test]
    fn falls_back_to_closest_newer() {
        let runtimes = [(17, "17.0.2"), (21, "21.0.8")];
        assert_eq!(pick(&runtimes, at_least(21)), Some("21.0.8"));
    }

    #[test]
    fn keeps_highest_legacy_runtime() {
        let runtimes = [(8, "8u402"), (8, "8u282")];
        let requirement = JavaRequirement {
            minimum_major_version: 8,
            maximum_major_version: Some(8),
        };
        assert_eq!(pick(&runtimes, requirement), Some("8u402"));
    }

    #[test]
    fn rejects_incompatible() {
        let runtimes = [(21, "21.0.8")];
        assert_eq!(pick(&runtimes, at_least(25)), None);
    }
}
