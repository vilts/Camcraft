// Provenance: original CamCraft catalog refusal case (#2856).
mod catalog_2d_drill_overfeed {
    use super::*;
    #[test]
    fn regenerates() {
        run_case("2d", "drill-overfeed", "regenerates", regenerate);
    }
    #[test]
    fn rebuilds() {
        run_case("2d", "drill-overfeed", "rebuilds", rebuild);
    }
}
