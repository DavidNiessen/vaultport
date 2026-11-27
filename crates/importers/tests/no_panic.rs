// SPDX-License-Identifier: GPL-3.0-or-later
//! Property tests: no parser panics on arbitrary input, and every bundle is consistent.

#![allow(clippy::unwrap_used)]

use core_model::Validate;
use importers::{
    Format, ImportBundle, csv, detect_format, import, known_hosts, mobaxterm, putty, securecrt,
    ssh_config,
};
use proptest::prelude::*;
use std::collections::HashSet;

fn check(bundle: &ImportBundle) {
    let ids: HashSet<_> = bundle.hosts.iter().map(|h| h.id).collect();
    for h in &bundle.hosts {
        assert!(h.validate().is_ok(), "invalid host {h:?}");
        for hop in h.config.ssh.jump_chain.iter().flatten() {
            assert!(ids.contains(hop), "dangling jump host");
            assert_ne!(*hop, h.id, "host jumps through itself");
        }
    }
    for f in &bundle.forwards {
        assert!(ids.contains(&f.host), "dangling forward");
        assert!(f.validate().is_ok());
    }
    for g in &bundle.groups {
        assert!(g.validate().is_ok());
    }
}

/// Lines assembled from fragments that exercise the interesting parser paths.
fn lines(vocab: &'static [&'static str], max: usize) -> impl Strategy<Value = String> {
    prop::collection::vec(
        prop::collection::vec(prop::sample::select(vocab), 0..6).prop_map(|t| t.join(" ")),
        0..max,
    )
    .prop_map(|l| l.join("\n"))
}

const SSH_VOCAB: &[&str] = &[
    "Host",
    "Match",
    "Include",
    "HostName",
    "User",
    "Port",
    "IdentityFile",
    "ProxyJump",
    "ProxyCommand",
    "ForwardAgent",
    "LocalForward",
    "RemoteForward",
    "DynamicForward",
    "SetEnv",
    "SendEnv",
    "Compression",
    "StrictHostKeyChecking",
    "=",
    "*",
    "a",
    "b",
    "c.example",
    "!a",
    "*.example",
    "%h",
    "%r",
    "u@a:22",
    "a,b",
    "b,a",
    "c.example,a",
    "\"x y\"",
    "\"",
    "'",
    "8080",
    "0",
    "[::1]:1",
    "localhost:80",
    "none",
    "yes",
    "no",
    "A=1",
    "#",
    "all",
    "\\",
];

const KH_VOCAB: &[&str] = &[
    "@cert-authority",
    "@revoked",
    "@x",
    "host",
    "a,b",
    "[h]:22",
    "|1|AAAA|BBBB",
    "ssh-ed25519",
    "ssh-rsa",
    "1024",
    "AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl",
    "AAAA",
    "#",
];

const REG_VOCAB: &[&str] = &[
    "[HKEY_CURRENT_USER\\Software\\SimonTatham\\PuTTY\\Sessions\\s%20x]",
    "[HKEY_CURRENT_USER\\Software\\SimonTatham\\PuTTY\\Sessions\\b]",
    "[HKEY_CURRENT_USER\\Software\\SimonTatham\\PuTTY\\SshHostKeys]",
    "[-HKEY_CURRENT_USER\\x]",
    "\"HostName\"=\"h.example\"",
    "\"HostName\"=\"u@b\"",
    "\"Protocol\"=\"telnet\"",
    "\"PortNumber\"=dword:00000016",
    "\"ProxyMethod\"=dword:00000006",
    "\"ProxyMethod\"=dword:00000002",
    "\"ProxyHost\"=\"b\"",
    "\"ProxyPassword\"=\"p\"",
    "\"PortForwardings\"=\"L1=a:2,D3,R4=::1:5,X\"",
    "\"rsa2@22:h\"=\"0x3,0xff\"",
    "\"ssh-ed25519@2:h\"=\"0x1,0x2\"",
    "hex:00,\\",
    "\"",
    "=",
];

const MOBA_VOCAB: &[&str] = &[
    "[Bookmarks]",
    "[Bookmarks_1]",
    "[Passwords]",
    "SubRep=",
    "SubRep=A\\B",
    "SubRep=\\\\",
    "s=#109#0%h.example%22%u%%-1%-1%%%%%0%0%0%key.ppk",
    "t=#98#1%h%23",
    "x=#91#0%h",
    "y=#abc",
    "z=#",
    "%",
    "#",
];

const XML_VOCAB: &[&str] = &[
    "<VanDyke>",
    "</VanDyke>",
    "<key name=\"Sessions\">",
    "<key name=\"f\">",
    "</key>",
    "<key name=\"x\"/>",
    "<string name=\"Hostname\">h.example</string>",
    "<string name=\"Protocol Name\">Telnet</string>",
    "<dword name=\"[SSH2] Port\">22</dword>",
    "<string name=\"Password V2\">x</string>",
    "&amp;",
    "&#x41;",
    "<![CDATA[x]]>",
    "<!-- c -->",
    "<array>",
    "</array>",
    "<",
];

const CSV_VOCAB: &[&str] = &[
    "label,address,port,username,password,group,tags,protocol",
    "a,h.example,22,u,p,A/B,t;u,ssh",
    "b,10.0.0.1,,,,,,telnet",
    ",,,,,,,",
    "\"x,y\",h",
    "\"",
    "c,bad host,99999",
    "d,h,1,,,,,rdp",
];

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn parsers_never_panic_on_arbitrary_text(s in any::<String>()) {
        for b in [
            ssh_config::parse(&s),
            known_hosts::parse(&s),
            csv::parse(&s),
            putty::parse(&s),
            mobaxterm::parse(&s),
            securecrt::parse(&s),
        ]
        .into_iter()
        .flatten()
        {
            check(&b);
        }
        let _ = ssh_config::include_paths(&s);
    }

    #[test]
    fn import_never_panics_on_arbitrary_bytes(bytes in prop::collection::vec(any::<u8>(), 0..2048)) {
        let _ = detect_format(&bytes);
        for f in Format::ALL {
            if let Ok(b) = import(f, &bytes) {
                check(&b);
            }
        }
    }

    #[test]
    fn ssh_config_structured(s in lines(SSH_VOCAB, 40), inc in lines(SSH_VOCAB, 10)) {
        let b = ssh_config::parse_with_includes(&s, |_| Some(inc.clone())).unwrap();
        check(&b);
    }

    #[test]
    fn known_hosts_structured(s in lines(KH_VOCAB, 30)) {
        check(&known_hosts::parse(&s).unwrap());
    }

    #[test]
    fn putty_structured(s in lines(REG_VOCAB, 40)) {
        let input = format!("Windows Registry Editor Version 5.00\n{s}");
        check(&putty::parse(&input).unwrap());
    }

    #[test]
    fn mobaxterm_structured(s in lines(MOBA_VOCAB, 40)) {
        let input = format!("[Bookmarks]\n{s}");
        check(&mobaxterm::parse(&input).unwrap());
    }

    #[test]
    fn securecrt_structured(s in lines(XML_VOCAB, 40)) {
        if let Ok(b) = securecrt::parse(&s) {
            check(&b);
        }
    }

    #[test]
    fn csv_structured(s in lines(CSV_VOCAB, 30)) {
        if let Ok(b) = csv::parse(&s) {
            check(&b);
        }
    }
}
