// SPDX-License-Identifier: GPL-3.0-or-later
#![no_main]
use libfuzzer_sys::fuzz_target;
use term_intel::generators as g;

// Files read from a (possibly hostile) remote machine for completion values.
fuzz_target!(|data: &[u8]| {
    let s = String::from_utf8_lossy(data);
    let _ = g::package_scripts(&s);
    let _ = g::deno_tasks(&s);
    let _ = g::make_targets(&s);
    let _ = g::just_recipes(&s);
    let _ = g::cargo_targets(&s);
    let _ = g::passwd_users(&s);
    let _ = g::group_names(&s);
    let _ = g::ssh_config_hosts(&s);
    let _ = g::known_hosts_names(&s);
    let _ = g::etc_hosts(&s);
    let _ = g::service_names(s.lines());
    let refs = g::git_refs(&g::GitFiles {
        head: s.to_string(),
        packed_refs: s.to_string(),
        config: s.to_string(),
        heads: s.lines().map(str::to_owned).collect(),
        remotes: Vec::new(),
        tags: Vec::new(),
    });
    let _ = g::git_values(&refs, term_intel::ArgKind::Ref);
});
