use crate::error::CliError;
use nsb::ComponentMask;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StarlightSelection {
    Production,
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
                starlight = Some(StarlightSelection::Production);
                mask |= ComponentMask::STARLIGHT;
            }
            "airglow" | "ag" => mask |= ComponentMask::AIRGLOW,
            "moon" | "moonlight" => mask |= ComponentMask::MOON,
            other => return Err(CliError::UnknownComponent(other.to_string())),
        }
    }
    Ok(ParsedComponents { mask, starlight })
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
        assert_eq!(
            mask.contains(ComponentMask::STARLIGHT),
            nsb::Starlight::bundled_production_available()
        );
    }

    #[test]
    fn starlight_selects_production() {
        assert_eq!(
            parse_components("starlight").unwrap().starlight,
            Some(StarlightSelection::Production)
        );
        assert!(parse_components("experimental-starlight").is_err());
    }
}
