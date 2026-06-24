use crate::error::CliError;
use nsb::ComponentMask;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StarlightSelection {
    ValidatedExternal,
    ExperimentalSeed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParsedComponents {
    pub mask: ComponentMask,
    pub starlight: Option<StarlightSelection>,
}

pub fn parse_components(input: &str) -> Result<ParsedComponents, CliError> {
    let mut mask = ComponentMask::empty();
    let mut starlight = None;
    for token in input.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        match token.to_ascii_lowercase().as_str() {
            "all" => mask |= ComponentMask::ALL,
            "zodiacal" | "zl" => mask |= ComponentMask::ZODIACAL,
            "starlight" => {
                select_starlight(&mut starlight, StarlightSelection::ValidatedExternal)?;
                mask |= ComponentMask::STARLIGHT;
            }
            "experimental-starlight" | "experimental-sl" => {
                select_starlight(&mut starlight, StarlightSelection::ExperimentalSeed)?;
                mask |= ComponentMask::STARLIGHT;
            }
            "airglow" | "ag" => mask |= ComponentMask::AIRGLOW,
            "moon" | "moonlight" => mask |= ComponentMask::MOON,
            other => return Err(CliError::UnknownComponent(other.to_string())),
        }
    }
    Ok(ParsedComponents { mask, starlight })
}

fn select_starlight(
    selected: &mut Option<StarlightSelection>,
    requested: StarlightSelection,
) -> Result<(), CliError> {
    if selected.is_some_and(|value| value != requested) {
        return Err(CliError::InvalidComponentSelection(
            "starlight and experimental-starlight cannot be combined".to_string(),
        ));
    }
    *selected = Some(requested);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_component_list() {
        let mask = parse_components("zodiacal,airglow,moon").unwrap().mask;
        assert!(mask.contains(ComponentMask::ZODIACAL));
        assert!(mask.contains(ComponentMask::AIRGLOW));
        assert!(mask.contains(ComponentMask::MOON));
        assert!(!mask.contains(ComponentMask::STARLIGHT));
    }

    #[test]
    fn parses_all_as_production_safe_components() {
        let mask = parse_components("all").unwrap().mask;
        assert!(mask.contains(ComponentMask::ZODIACAL));
        assert!(mask.contains(ComponentMask::AIRGLOW));
        assert!(mask.contains(ComponentMask::MOON));
        assert!(!mask.contains(ComponentMask::STARLIGHT));
    }

    #[test]
    fn all_can_be_combined_with_explicit_experimental_starlight() {
        let mask = parse_components("all,experimental-starlight").unwrap().mask;
        assert!(mask.contains(ComponentMask::ZODIACAL));
        assert!(mask.contains(ComponentMask::AIRGLOW));
        assert!(mask.contains(ComponentMask::MOON));
        assert!(mask.contains(ComponentMask::STARLIGHT));
    }

    #[test]
    fn starlight_names_select_distinct_modes() {
        assert_eq!(
            parse_components("starlight").unwrap().starlight,
            Some(StarlightSelection::ValidatedExternal)
        );
        assert_eq!(
            parse_components("experimental-starlight")
                .unwrap()
                .starlight,
            Some(StarlightSelection::ExperimentalSeed)
        );
        assert!(parse_components("starlight,experimental-starlight").is_err());
    }
}
