fn main() {
    fn version_matches(requested: &str, existing: &str) -> bool {
        let requested_clean = requested.trim_start_matches('v');
        let existing_clean = existing.trim_start_matches('v');

        if requested_clean == existing_clean {
            return true;
        }

        let req_parse = semver::Version::parse(requested_clean);
        let exist_parse = semver::Version::parse(existing_clean);

        if let (Ok(req_semver), Ok(exist_semver)) = (req_parse, exist_parse) {
            let req_parts = requested_clean.split('.').count();
            if req_parts <= 2 {
                req_semver.major == exist_semver.major && req_semver.minor == exist_semver.minor
            } else {
                req_semver == exist_semver
            }
        } else {
            if requested_clean.split('.').count() <= 2 {
                if let Ok(req_req) = semver::VersionReq::parse(requested_clean) {
                    if let Ok(exist_semver) = semver::Version::parse(existing_clean) {
                        return req_req.matches(&exist_semver);
                    }
                }
            }
            requested_clean == existing_clean
        }
    }
    
    println!("1.2 vs 1.3.0: {}", version_matches("1.2", "1.3.0"));
}
