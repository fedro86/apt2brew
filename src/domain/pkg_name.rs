/// Strict validator for package names passed to `brew` / `apt` / `sudo apt` argv.
///
/// Guards against an untrusted rollback script or corrupted dpkg/Homebrew data
/// injecting an apt/brew option (e.g. `-o APT::Get::AllowUnauthenticated=true`)
/// disguised as a package name. Combined with a `--` end-of-options argv marker
/// at every destructive call site, this closes the flag-injection class.
///
/// Accepts the character set used by real Debian and Homebrew names:
/// ASCII alphanumerics plus `+ . - _ @`, must start with alphanumeric.
pub fn is_valid_package_name(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    let mut chars = s.chars();
    let first = chars.next().unwrap();
    if !first.is_ascii_alphanumeric() {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-' | '_' | '@'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_common_debian_names() {
        assert!(is_valid_package_name("git"));
        assert!(is_valid_package_name("libc6"));
        assert!(is_valid_package_name("lib2geom-1.2-1"));
        assert!(is_valid_package_name("python3.11"));
        assert!(is_valid_package_name("g++"));
        assert!(is_valid_package_name("gcc-13"));
        assert!(is_valid_package_name("linux-image-6.8.0-107-generic"));
        assert!(is_valid_package_name("openjdk-21-jre-headless"));
    }

    #[test]
    fn accepts_common_homebrew_names() {
        assert!(is_valid_package_name("node"));
        assert!(is_valid_package_name("python@3.12"));
        assert!(is_valid_package_name("fd"));
        assert!(is_valid_package_name("pkg-config"));
    }

    #[test]
    fn rejects_leading_dash() {
        assert!(!is_valid_package_name("-o"));
        assert!(!is_valid_package_name("--reinstall"));
        assert!(!is_valid_package_name("-"));
    }

    #[test]
    fn rejects_shell_metacharacters() {
        assert!(!is_valid_package_name("git; rm -rf /"));
        assert!(!is_valid_package_name("git`id`"));
        assert!(!is_valid_package_name("git$(id)"));
        assert!(!is_valid_package_name("git|cat"));
        assert!(!is_valid_package_name("git&sleep 5"));
    }

    #[test]
    fn rejects_whitespace_and_empty() {
        assert!(!is_valid_package_name(""));
        assert!(!is_valid_package_name(" "));
        assert!(!is_valid_package_name("git foo"));
        assert!(!is_valid_package_name("\tgit"));
        assert!(!is_valid_package_name("git\n"));
    }

    #[test]
    fn rejects_path_traversal() {
        assert!(!is_valid_package_name("../etc/passwd"));
        assert!(!is_valid_package_name("/usr/bin/evil"));
    }

    #[test]
    fn rejects_apt_option_forms() {
        assert!(!is_valid_package_name("--reinstall=evil"));
        assert!(!is_valid_package_name(
            "-oAPT::Get::AllowUnauthenticated=true"
        ));
    }
}
