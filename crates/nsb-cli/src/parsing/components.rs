use crate::error::CliError;
use nsb::ComponentMask;

pub fn parse_components(input: &str) -> Result<ComponentMask, CliError> {
    let mut mask = ComponentMask::empty();
    for token in input.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        match token.to_ascii_lowercase().as_str() {
            "all" => return Ok(ComponentMask::ALL),
            "zodiacal" | "zl" => mask |= ComponentMask::ZODIACAL,
            "experimental-starlight" | "experimental-sl" => mask |= ComponentMask::STARLIGHT,
            "airglow" | "ag" => mask |= ComponentMask::AIRGLOW,
            "moon" | "moonlight" => mask |= ComponentMask::MOON,
            other => return Err(CliError::UnknownComponent(other.to_string())),
        }
    }
    Ok(mask)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_component_list() {
        let mask = parse_components("zodiacal,airglow,moon").unwrap();
        assert!(mask.contains(ComponentMask::ZODIACAL));
        assert!(mask.contains(ComponentMask::AIRGLOW));
        assert!(mask.contains(ComponentMask::MOON));
        assert!(!mask.contains(ComponentMask::STARLIGHT));
    }

    #[test]
    fn parses_all_as_production_safe_components() {
        let mask = parse_components("all").unwrap();
        assert!(mask.contains(ComponentMask::ZODIACAL));
        assert!(mask.contains(ComponentMask::AIRGLOW));
        assert!(mask.contains(ComponentMask::MOON));
        assert!(!mask.contains(ComponentMask::STARLIGHT));
    }

    #[test]
    fn experimental_starlight_requires_explicit_name() {
        assert!(parse_components("starlight").is_err());
        assert_eq!(
            parse_components("experimental-starlight").unwrap(),
            ComponentMask::STARLIGHT
        );
    }
}
