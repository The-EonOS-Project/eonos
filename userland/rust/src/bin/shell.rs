#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use eonos_user::{Error, File, Process};

const MAX_JOBS: usize = 32;
const MAX_SCRIPT_DEPTH: usize = 16;
const OP_PIPE: &str = "\0|";
const OP_INPUT: &str = "\0<";
const OP_OUTPUT: &str = "\0>";
const OP_APPEND: &str = "\0>>";

struct Job {
    process: Process,
    command: String,
    status: Option<i32>,
}

fn main(args: eonos_rt::Args<'_>) -> i32 {
    let mut environment: Vec<String> = args
        .envs()
        .filter_map(|value| value.to_str().ok())
        .map(String::from)
        .collect();
    
    
    let argv: Vec<String> = args.skip(1).map(|value| value.to_str().unwrap_or("").to_string()).collect();
    if env_value(&environment, "PATH").is_none() {
        set_env(&mut environment, "PATH", "/disk/apps/bin:/bin");
    }
    set_env(&mut environment, "USER", "superuser");
    set_env(&mut environment, "HOSTNAME", "eonos");
    if let Ok(path) = eonos_user::current_dir() {
        set_env(&mut environment, "PWD", &path);
    }
    let mut jobs = Vec::new();
    if argv.first().is_some_and(|argument| argument == "-c") {
        let Some(command) = argv.get(1) else {
            eonos_rt::println!("usage: rust-shell -c <command>");
            return 2;
        };
        match run_line(command, &mut jobs, &mut environment, 0) {
            Ok(_) => {}
            Err(error) => eonos_rt::println!("error: {}", error),
        }
        if let Ok(path) = eonos_user::current_dir() {
            set_env(&mut environment, "PWD", &path);
        }
        eonos_rt::println!("\x1eKONSUL_STATE_BEGIN");
        for entry in &environment { eonos_rt::println!("{}", entry); }
        eonos_rt::println!("\x1eKONSUL_STATE_END");
        return 0;
    }
    eonos_rt::println!("EonOS shell -- type 'help' for commands");
    eonos_rt::println!("Welcome to EonOS!");
    eonos_rt::println!("Mount a writable disk at /disk to keep files and installed apps across restarts.");
    eonos_rt::println!("Crash reports are saved there too. Use `lsblk` to find a disk, then `mount <device> /disk`.");
    match run_script("/etc/profile", &mut jobs, &mut environment, 0) {
        Ok(true) => return 0,
        Ok(false) => {}
        Err(error) => eonos_rt::println!("profile: {}", error),
    }
    loop {
        reap_jobs(&mut jobs);
        let cwd = eonos_user::current_dir().unwrap_or_else(|_| String::from("/"));
        let user = env_value(&environment, "USER").unwrap_or("superuser");
        let host = env_value(&environment, "HOSTNAME").unwrap_or("eonos");
        let _ = eonos_user::print(&alloc::format!("{}@{}:{}# ", user, host, cwd));
        match eonos_user::read_line(0) {
            Ok(Some(line)) => match run_line(&line, &mut jobs, &mut environment, 0) {
                Ok(true) => return 0,
                Ok(false) => {}
                Err(error) => eonos_rt::println!("error: {}", error),
            },
            Ok(None) => return 0,
            Err(error) => eonos_rt::println!("input error: {}", error),
        }
    }
}

fn run_line(
    line: &str,
    jobs: &mut Vec<Job>,
    environment: &mut Vec<String>,
    script_depth: usize,
) -> Result<bool, Error> {
    let words = parse_words(line, environment)?;
    if words
        .iter()
        .any(|word| matches!(word.as_str(), OP_PIPE | OP_INPUT | OP_OUTPUT | OP_APPEND))
    {
        execute_pipeline(&words, jobs, environment)?;
        return Ok(false);
    }
    let Some(command) = words.first().map(String::as_str) else {
        return Ok(false);
    };
    let args = &words[1..];

    match command {
        "era" if env_value(environment, "EONOS_ERA_SESSION").is_some() => {
            eonos_rt::println!("ERROR : Era Session Already Running! (EC : ESAR)");
        }
        "help" => {
            ensure_no_args(args, "help")?;
            print_help();
        }
        "env" => {
            ensure_no_args(args, "env")?;
            for entry in environment.iter() {
                eonos_rt::println!("{}", entry);
            }
        }
        "export" => export_variables(args, environment)?,
        "unset" => unset_variables(args, environment)?,
        "echo" => {
            let no_newline = args.first().is_some_and(|arg| arg == "-n");
            let text = args[usize::from(no_newline)..].join(" ");
            eonos_user::print(&text)?;
            if !no_newline { eonos_user::print("\n")?; }
        }
        "ls" => {
            if args.len() > 1 { return Err(Error::Usage("ls [directory]")); }
            let path = args.first().map(String::as_str).unwrap_or(".");
            print_listing(eonos_user::list_directory(path)?)?;
        }
        "lsblk" => {
            ensure_no_args(args, "lsblk")?;
            print_listing(eonos_user::list_block_devices()?)?;
        }
        "mounts" => {
            ensure_no_args(args, "mounts")?;
            print_listing(eonos_user::list_mounts()?)?;
        }
        "devinfo" => {
            ensure_no_args(args, "devinfo")?;
            let devices = eonos_user::list_block_devices()?;
            eonos_rt::println!("Block devices:");
            print_listing(devices)?;
            match eonos_rt::network_info() {
                Ok(info) => eonos_rt::println!(
                    "Network e1000: link {}, {} RX / {} TX frames",
                    if info.link_up != 0 { "up" } else { "down" },
                    info.received, info.transmitted
                ),
                Err(_) => eonos_rt::println!("Network e1000: unavailable"),
            }
            match eonos_rt::mouse_state() {
                Ok(state) => eonos_rt::println!(
                    "USB mouse: x={} y={} buttons={:#04x}", state.x, state.y, state.buttons
                ),
                Err(_) => eonos_rt::println!("USB mouse: unavailable"),
            }
        }
        "mouse" => {
            ensure_no_args(args, "mouse")?;
            let state = eonos_rt::mouse_state()?;
            eonos_rt::println!(
                "x={} y={} buttons={:#04x} wheel={}",
                state.x, state.y, state.buttons, state.wheel
            );
        }
        "net" => {
            if args.len() == 1 && args[0] == "recv" {
                let mut frame = [0u8; 1518];
                let length = eonos_rt::receive_ethernet_frame(&mut frame)?;
                let ethertype = u16::from_be_bytes([frame[12], frame[13]]);
                eonos_rt::println!("received Ethernet frame: {} bytes, type {:#06x}", length, ethertype);
            } else if args.first().is_some_and(|arg| arg == "udp-send") {
                if args.len() < 4 { return Err(Error::Usage("net udp-send <ip> <port> <source-port> <text...>")); }
                let destination = parse_ipv4(&args[1])?;
                let port = args[2].parse::<u16>().map_err(|_| Error::InvalidArgument)?;
                let source_port = args[3].parse::<u16>().map_err(|_| Error::InvalidArgument)?;
                let payload = args[4..].join(" ");
                let sent = eonos_rt::udp_send(destination, port, source_port, payload.as_bytes())?;
                eonos_rt::println!("UDP: sent {} bytes to {}:{} from port {}", sent, args[1], port, source_port);
            } else if args.first().is_some_and(|arg| arg == "udp-recv") {
                if args.len() > 2 { return Err(Error::Usage("net udp-recv [max-bytes]")); }
                let capacity = args.get(1).map(|arg| arg.parse::<usize>().map_err(|_| Error::InvalidArgument)).transpose()?.unwrap_or(512).min(1472);
                let mut packet = eonos_rt::UdpDatagramInfo::default();
                let mut payload = vec![0u8; capacity];
                let length = eonos_rt::udp_receive(&mut packet, &mut payload)?;
                eonos_rt::println!("UDP: {} bytes from {}.{}.{}.{}:{}", length, packet.source_ip[0], packet.source_ip[1], packet.source_ip[2], packet.source_ip[3], packet.source_port);
                match core::str::from_utf8(&payload[..length]) {
                    Ok(text) => eonos_rt::println!("{}", text),
                    Err(_) => {
                        for byte in &payload[..length] { eonos_user::print(&format!("{:02x} ", byte))?; }
                        eonos_user::print("\n")?;
                    }
                }
            } else if args.len() == 1 && args[0] == "dhcp" {
                eonos_rt::println!("DHCP: requesting an IPv4 lease (up to 10 seconds)");
                eonos_rt::dhcp()?;
                let config = eonos_rt::ipv4_config()?;
                eonos_rt::println!("address: {}.{}.{}.{}", config.address[0], config.address[1], config.address[2], config.address[3]);
                eonos_rt::println!("netmask: {}.{}.{}.{}", config.netmask[0], config.netmask[1], config.netmask[2], config.netmask[3]);
                eonos_rt::println!("gateway: {}.{}.{}.{}", config.gateway[0], config.gateway[1], config.gateway[2], config.gateway[3]);
                eonos_rt::println!("DNS: {}.{}.{}.{}", config.dns_server[0], config.dns_server[1], config.dns_server[2], config.dns_server[3]);
                eonos_rt::println!("lease: {} seconds", config.lease_seconds);
            } else if args.is_empty() {
                let info = eonos_rt::network_info()?;
                let config = eonos_rt::ipv4_config()?;
                eonos_rt::println!("interface: e1000");
                eonos_rt::println!("link: {}", if info.link_up != 0 { "up" } else { "down" });
                eonos_rt::println!("MAC: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
                    info.mac[0], info.mac[1], info.mac[2], info.mac[3], info.mac[4], info.mac[5]);
                eonos_rt::println!("frames: {} received, {} transmitted", info.received, info.transmitted);
                if config.configured != 0 {
                    eonos_rt::println!("IPv4: {}.{}.{}.{} / {}.{}.{}.{} via {}.{}.{}.{}",
                        config.address[0], config.address[1], config.address[2], config.address[3],
                        config.netmask[0], config.netmask[1], config.netmask[2], config.netmask[3],
                        config.gateway[0], config.gateway[1], config.gateway[2], config.gateway[3]);
                    eonos_rt::println!("DNS: {}.{}.{}.{}", config.dns_server[0], config.dns_server[1], config.dns_server[2], config.dns_server[3]);
                } else { eonos_rt::println!("IPv4: unconfigured (run `net dhcp`)"); }
            } else {
                return Err(Error::Usage("net [dhcp|recv|udp-send|udp-recv]"));
            }
        }
        "nslookup" => {
            exact_args(args, 1, "nslookup <hostname>")?;
            let address = resolve_host(required(args, 0, "nslookup <hostname>")?)?;
            eonos_rt::println!("{} -> {}.{}.{}.{}", args[0], address[0], address[1], address[2], address[3]);
        }
        "ping" => {
            if args.is_empty() || args.len() > 2 { return Err(Error::Usage("ping <ip> [count]")); }
            let destination = resolve_host(&args[0])?;
            let count = args.get(1).map(|arg| arg.parse::<usize>().map_err(|_| Error::InvalidArgument)).transpose()?.unwrap_or(4);
            if count == 0 || count > 20 { return Err(Error::InvalidArgument); }
            eonos_rt::println!("PING {} ({}.{}.{}.{}): {} packets", args[0], destination[0], destination[1], destination[2], destination[3], count);
            let mut received = 0usize;
            let mut total_ms = 0u64;
            for sequence in 1..=count {
                match eonos_rt::ping(destination, 1000)? {
                    Some(elapsed) => {
                        received += 1;
                        total_ms = total_ms.saturating_add(elapsed);
                        eonos_rt::println!("reply from {}: seq={} time={} ms", args[0], sequence, elapsed);
                    }
                    None => eonos_rt::println!("timeout from {}: seq={}", args[0], sequence),
                }
                if sequence != count { eonos_rt::sleep_ms(1000)?; }
            }
            if received != 0 {
                eonos_rt::println!("{}: {}/{} replies, average {} ms", args[0], received, count, total_ms / received as u64);
            } else {
                eonos_rt::println!("{}: 0/{} replies", args[0], count);
            }
        }
        "ps" => {
            ensure_no_args(args, "ps")?;
            print_listing(eonos_user::list_processes()?)?;
        }
        "cat" => {
            if args.is_empty() { return Err(Error::Usage("cat <file>...")); }
            for path in args { cat(path)?; }
        }
        "head" => head(args)?,
        "wc" => word_count(args)?,
        "tree" => {
            if args.len() > 1 { return Err(Error::Usage("tree [directory]")); }
            let root = args.first().map(String::as_str).unwrap_or(".");
            eonos_rt::println!("{}", root);
            let mut seen = 0usize;
            tree_walk(root, 0, &mut seen)?;
        }
        "which" => {
            exact_args(args, 1, "which <command>")?;
            let name = required(args, 0, "which <command>")?;
            match executable_path(name, environment) {
                Some(path) => eonos_rt::println!("{}", path),
                None => eonos_rt::println!("{}: command not found", name),
            }
        }
        "source" | "." | "run" => {
            exact_args(args, 1, "source <file>")?;
            return run_script(required(args, 0, "source <file>")?, jobs, environment, script_depth + 1);
        }
        "exec" => run_child(args, jobs, environment)?,
        "write" => write_file(args)?,
        "mkdir" => with_path(args, "mkdir <directory>", eonos_user::mkdir)?,
        "rm" => with_path(args, "rm <file-or-empty-directory>", eonos_user::remove)?,
        "touch" => {
            exact_args(args, 1, "touch <file>")?;
            touch(required(args, 0, "touch <file>")?)?;
        }
        "cp" => copy_file(args)?,
        "edit" => {
            exact_args(args, 1, "edit <file>")?;
            edit_file(required(args, 0, "edit <file>")?)?;
        }
        "mount" => {
            exact_args(args, 2, "mount <device> <directory>")?;
            mount_device(args)?;
        }
        "install" => {
            exact_args(args, 1, "install <package.eonapp>")?;
            let package = required(args, 0, "install <package.eonapp>")?;
            if !eonos_user::is_mountpoint("/disk")? {
                eonos_rt::println!("install: mount a writable data disk at /disk first");
            } else {
                install_app_package(package)?;
            }
        }
        "uninstall" => {
            exact_args(args, 1, "uninstall <app-id>")?;
            let id = required(args, 0, "uninstall <app-id>")?;
            if !eonos_user::is_mountpoint("/disk")? {
                eonos_rt::println!("uninstall: mount a writable data disk at /disk first");
            } else {
                uninstall_app(id)?;
            }
        }
        "jobs" => list_jobs(jobs)?,
        "wait" => wait_jobs(args, jobs)?,
        "kill" => kill_job(args, jobs)?,
        "sleep" => {
            exact_args(args, 1, "sleep <milliseconds>")?;
            let milliseconds = required(args, 0, "sleep <milliseconds>")?
                .parse::<usize>()
                .map_err(|_| Error::InvalidArgument)?;
            eonos_rt::sleep_ms(milliseconds)?;
        }
        "pwd" => {
            ensure_no_args(args, "pwd")?;
            eonos_rt::println!("{}", eonos_user::current_dir()?);
        }
        "cd" => {
            if args.len() > 1 { return Err(Error::Usage("cd [directory]")); }
            eonos_user::chdir(args.first().map(String::as_str).unwrap_or("/"))?;
            let path = eonos_user::current_dir()?;
            set_env(environment, "PWD", &path);
        }
        "uptime" => {
            ensure_no_args(args, "uptime")?;
            eonos_rt::println!("{} ms", eonos_rt::uptime_ms());
        }
        "free" => {
            ensure_no_args(args, "free")?;
            let memory = eonos_rt::memory_info()?;
            eonos_rt::println!("Physical memory free: {} MiB", memory.free_physical / (1024 * 1024));
            eonos_rt::println!("Kernel heap: {} KiB mapped, {} KiB used, {} KiB available", memory.heap_total / 1024, memory.heap_used / 1024, memory.heap_total.saturating_sub(memory.heap_used) / 1024);
        }
        "mem" => {
            ensure_no_args(args, "mem")?;
            let memory = eonos_rt::memory_info()?;
            let physical_used = memory.total_physical.saturating_sub(memory.free_physical);
            eonos_rt::println!("Allocatable physical memory: {} bytes total, {} used, {} free", memory.total_physical, physical_used, memory.free_physical);
            eonos_rt::println!("Kernel heap: {} bytes used / {} bytes mapped ({} bytes free)", memory.heap_used, memory.heap_total, memory.heap_total.saturating_sub(memory.heap_used));
            eonos_rt::println!("Process: pid {}", eonos_rt::getpid());
        }
        "kdiag" => {
            ensure_no_args(args, "kdiag")?;
            eonos_rt::println!("EonOS diagnostic snapshot at {} ms", eonos_rt::uptime_ms());
            eonos_rt::println!("Current process: pid {}", eonos_rt::getpid());
            let memory = eonos_rt::memory_info()?;
            eonos_rt::println!("Memory: {} MiB physical free; kernel heap {} / {} KiB used", memory.free_physical / (1024 * 1024), memory.heap_used / 1024, memory.heap_total / 1024);
            eonos_rt::println!("Processes:");
            print_listing(eonos_user::list_processes()?)?;
            eonos_rt::println!("Storage devices:");
            print_listing(eonos_user::list_block_devices()?)?;
            eonos_rt::println!("Filesystem mounts:");
            print_listing(eonos_user::list_mounts()?)?;
            if let Ok(info) = eonos_rt::network_info() {
                eonos_rt::println!("E1000: link {}, {} RX / {} TX frames", if info.link_up != 0 { "up" } else { "down" }, info.received, info.transmitted);
            } else {
                eonos_rt::println!("E1000: unavailable");
            }
        }
        "pid" => {
            ensure_no_args(args, "pid")?;
            eonos_rt::println!("{}", eonos_rt::getpid());
        }
        "sync" => {
            ensure_no_args(args, "sync")?;
            eonos_rt::sync()?;
            eonos_rt::println!("sync: storage reports durable writes");
        }
        "reboot" => {
            ensure_no_args(args, "reboot")?;
            eonos_rt::println!("reboot: syncing storage and restarting");
            eonos_rt::reboot()?;
        }
        "shutdown" | "poweroff" => {
            ensure_no_args(args, "shutdown")?;
            eonos_rt::println!("shutdown: syncing storage and powering off");
            eonos_rt::shutdown()?;
        }
        "clear" => {
            ensure_no_args(args, "clear")?;
            eonos_user::print("\x1b[2J\x1b[H")?;
        }
        "panic" => {
            ensure_no_args(args, "panic")?;
            eonos_rt::panic_kernel();
        }
        "exit" => {
            ensure_no_args(args, "exit")?;
            return Ok(true);
        }
        _ => {
            let Some(path) = executable_path(command, environment) else {
                eonos_rt::println!("{}: command not found", command);
                return Ok(false);
            };
            match launch_program(&path, args, jobs, environment, line) {
                Ok(()) => {}
                Err(Error::System(eonos_rt::Error(2))) => eonos_rt::println!("{}: command not found", command),
                Err(error) => return Err(error),
            }
        }
    }
    Ok(false)
}

fn ensure_no_args(args: &[String], command: &'static str) -> Result<(), Error> {
    if args.is_empty() {
        Ok(())
    } else {
        Err(Error::Usage(command))
    }
}

fn exact_args(args: &[String], count: usize, usage: &'static str) -> Result<(), Error> {
    if args.len() == count {
        Ok(())
    } else {
        Err(Error::Usage(usage))
    }
}

fn parse_words(line: &str, environment: &[String]) -> Result<Vec<String>, Error> {
    let characters: Vec<char> = line.chars().collect();
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quote = None;
    let mut escaped = false;
    let mut started = false;
    let mut index = 0;
    while index < characters.len() {
        let character = characters[index];
        if escaped {
            word.push(character);
            escaped = false;
            started = true;
            index += 1;
            continue;
        }
        if character == '\\' && quote != Some('\'') {
            escaped = true;
            started = true;
            index += 1;
            continue;
        }
        if let Some(delimiter) = quote {
            if character == delimiter {
                quote = None;
            } else if character == '$' && delimiter == '"' {
                let (value, next) = expand_variable(&characters, index, environment)?;
                word.push_str(value);
                index = next;
                started = true;
                continue;
            } else {
                word.push(character);
            }
            started = true;
            index += 1;
            continue;
        }
        match character {
            '\'' | '"' => {
                quote = Some(character);
                started = true;
                index += 1;
            }
            c if c.is_whitespace() => {
                if started {
                    words.push(core::mem::take(&mut word));
                    started = false;
                }
                index += 1;
            }
            '$' => {
                let (value, next) = expand_variable(&characters, index, environment)?;
                word.push_str(value);
                started = true;
                index = next;
            }
            '|' | '<' | '>' => {
                if started {
                    words.push(core::mem::take(&mut word));
                    started = false;
                }
                if character == '>' && characters.get(index + 1) == Some(&'>') {
                    words.push(String::from(OP_APPEND));
                    index += 2;
                } else {
                    words.push(String::from(match character {
                        '|' => OP_PIPE,
                        '<' => OP_INPUT,
                        _ => OP_OUTPUT,
                    }));
                    index += 1;
                }
            }
            '#' if !started => break,
            c => {
                word.push(c);
                started = true;
                index += 1;
            }
        }
    }
    if escaped || quote.is_some() {
        return Err(Error::InvalidInput);
    }
    if started {
        words.push(word);
    }
    Ok(words)
}

fn run_script(
    path: &str,
    jobs: &mut Vec<Job>,
    environment: &mut Vec<String>,
    depth: usize,
) -> Result<bool, Error> {
    if depth >= MAX_SCRIPT_DEPTH {
        return Err(Error::Usage("maximum script nesting exceeded"));
    }
    let mut file = match File::open(path, eonos_user::flags::READ_ONLY) {
        Ok(file) => file,
        Err(Error::System(eonos_rt::Error(2))) if path == "/etc/profile" => return Ok(false),
        Err(error) => return Err(error),
    };
    let contents = file.read_to_string()?;
    for (line_number, line) in contents.lines().enumerate() {
        match run_line(line, jobs, environment, depth) {
            Ok(true) => return Ok(true),
            Ok(false) => {}
            Err(error) => {
                eonos_rt::println!("{}:{}: {}", path, line_number + 1, error);
                return Ok(false);
            }
        }
    }
    Ok(false)
}

fn expand_variable<'a>(
    characters: &[char],
    start: usize,
    environment: &'a [String],
) -> Result<(&'a str, usize), Error> {
    let mut index = start + 1;
    let name = if characters.get(index) == Some(&'{') {
        index += 1;
        let name_start = index;
        while index < characters.len() && characters[index] != '}' {
            index += 1;
        }
        if index == characters.len() {
            return Err(Error::InvalidInput);
        }
        let name: String = characters[name_start..index].iter().collect();
        index += 1;
        name
    } else {
        if !characters
            .get(index)
            .is_some_and(|character| *character == '_' || character.is_ascii_alphabetic())
        {
            return Ok(("$", start + 1));
        }
        let name_start = index;
        while index < characters.len()
            && (characters[index] == '_' || characters[index].is_ascii_alphanumeric())
        {
            index += 1;
        }
        characters[name_start..index].iter().collect()
    };
    if !valid_env_name(&name) {
        return Err(Error::InvalidArgument);
    }
    Ok((env_value(environment, &name).unwrap_or(""), index))
}

fn required<'a>(args: &'a [String], index: usize, usage: &'static str) -> Result<&'a str, Error> {
    args.get(index)
        .map(String::as_str)
        .ok_or(Error::Usage(usage))
}

fn parse_ipv4(value: &str) -> Result<[u8; 4], Error> {
    let mut parts = value.split('.');
    let mut address = [0u8; 4];
    for octet in &mut address {
        *octet = parts.next().ok_or(Error::InvalidArgument)?
            .parse::<u8>().map_err(|_| Error::InvalidArgument)?;
    }
    if parts.next().is_some() { return Err(Error::InvalidArgument); }
    Ok(address)
}

fn resolve_host(value: &str) -> Result<[u8; 4], Error> {
    match parse_ipv4(value) {
        Ok(address) => Ok(address),
        Err(_) => Ok(eonos_rt::resolve_ipv4(value)?),
    }
}

fn with_path(
    args: &[String],
    usage: &'static str,
    operation: fn(&str) -> Result<(), Error>,
) -> Result<(), Error> {
    exact_args(args, 1, usage)?;
    operation(required(args, 0, usage)?)?;
    Ok(())
}

fn print_listing(listing: String) -> Result<(), Error> {
    eonos_user::print(&listing)?;
    if !listing.ends_with('\n') {
        eonos_user::print("\n")?;
    }
    Ok(())
}

fn print_help() {
    eonos_rt::println!("commands: cd [dir], pwd, ls [dir], tree [dir], cat <file>..., head [-n lines] <file>, wc <file>..., echo [-n] [text...], sleep <ms>, mouse, nslookup <hostname>, ping <host> [count], net [dhcp|recv|udp-send|udp-recv],");
    eonos_rt::println!("text filters: grep [-n] [-i] <pattern> [file...], tail [-n lines] [file]; tools accept pipes on standard input");
    eonos_rt::println!("curler <http[s]://host/path> [output-file] downloads HTTP/HTTPS files (HTTPS certificates are verified)");
    eonos_rt::println!("example: curler https://raw.githubusercontent.com/user/repo/main/app.eonapp /disk/app.eonapp; install /disk/app.eonapp");
    eonos_rt::println!("lsblk, mounts, mount <device> <dir>, install <package.eonapp>, uninstall <app-id>, exec <file> [args...] [&],");
    eonos_rt::println!("pipelines: command | command; input: command < file; output: command > file or >> file");
    eonos_rt::println!("environment: env, export NAME=value, unset NAME; expand $NAME or ${{NAME}}; PATH selects command directories");
    eonos_rt::println!("scripts: source <file> (or run <file>) executes lines in this shell; startup settings load from /etc/profile");
    eonos_rt::println!("jobs, wait [pid], kill <pid>,");
    eonos_rt::println!("write <file> <text>, mkdir, rm,");
    eonos_rt::println!("touch, cp <source> <destination>, edit <file>,");
    eonos_rt::println!("era opens the graphical desktop; other commands are searched for in /bin; Ctrl-C stops a foreground program");
    eonos_rt::println!("ps, free, mem, devinfo, kdiag, which <command>, clear, uptime, sync, reboot, shutdown, poweroff, pid, panic, help, exit");
}

fn head(args: &[String]) -> Result<(), Error> {
    let (count, path_index) = if args.first().is_some_and(|arg| arg == "-n") {
        let count = args.get(1).ok_or(Error::Usage("head [-n lines] <file>"))?
            .parse::<usize>().map_err(|_| Error::InvalidArgument)?;
        (count, 2)
    } else { (10, 0) };
    exact_args(&args[path_index..], 1, "head [-n lines] <file>")?;
    let path = required(args, path_index, "head [-n lines] <file>")?;
    let mut file = File::open(path, eonos_user::flags::READ_ONLY)?;
    let data = file.read_to_end()?;
    let end = if count == 0 { 0 } else {
        data.iter().enumerate().filter(|(_, byte)| **byte == b'\n')
            .nth(count - 1).map(|(index, _)| index + 1).unwrap_or(data.len())
    };
    eonos_rt::write_all(1, &data[..end])?;
    if end > 0 && data[end - 1] != b'\n' { eonos_rt::write_all(1, b"\n")?; }
    Ok(())
}

fn word_count(args: &[String]) -> Result<(), Error> {
    if args.is_empty() { return Err(Error::Usage("wc <file>...")); }
    for path in args {
        let mut file = File::open(path, eonos_user::flags::READ_ONLY)?;
        let bytes = file.read_to_end()?;
        let lines = bytes.iter().filter(|byte| **byte == b'\n').count();
        let words = core::str::from_utf8(&bytes).map_err(|_| Error::InvalidArgument)?
            .split_whitespace().count();
        eonos_rt::println!("{:>6} {:>6} {:>8} {}", lines, words, bytes.len(), path);
    }
    Ok(())
}

fn tree_walk(path: &str, depth: usize, seen: &mut usize) -> Result<(), Error> {
    if depth >= 8 || *seen >= 256 { return Ok(()); }
    let listing = eonos_user::list_directory(path)?;
    for name in listing.lines() {
        if *seen >= 256 { break; }
        *seen += 1;
        for _ in 0..depth { eonos_user::print("  ")?; }
        eonos_rt::println!("{}", name);
        if name.ends_with('/') {
            let child = if path == "/" { alloc::format!("/{}", &name[..name.len()-1]) }
                else if path == "." { alloc::format!("./{}", &name[..name.len()-1]) }
                else { alloc::format!("{}/{}", path.trim_end_matches('/'), &name[..name.len()-1]) };
            tree_walk(&child, depth + 1, seen)?;
        }
    }
    Ok(())
}

fn cat(path: &str) -> Result<(), Error> {
    let mut file = File::open(path, eonos_user::flags::READ_ONLY)?;
    eonos_rt::write_all(1, &file.read_to_end()?)?;
    Ok(())
}

fn run_child(
    args: &[String],
    jobs: &mut Vec<Job>,
    environment: &[String],
) -> Result<(), Error> {
    let path = required(args, 0, "exec <file> [args...]")?;
    launch_program(path, &args[1..], jobs, environment, &args.join(" "))
}

fn launch_program(
    path: &str,
    args: &[String],
    jobs: &mut Vec<Job>,
    environment: &[String],
    display: &str,
) -> Result<(), Error> {
    let background = args.last().is_some_and(|arg| arg == "&");
    let end = args.len() - usize::from(background);
    let child_args: Vec<_> = args[..end].iter().map(String::as_str).collect();
    let env_values: Vec<&str> = environment.iter().map(String::as_str).collect();
    if path == "/bin/era" && !eonos_user::is_mountpoint("/disk")? {
        eonos_rt::println!("ERROR: Era requires a writable data disk mounted at /disk.");
        return Ok(());
    }
    let app_type = packaged_app_type(path);
    if app_type.as_deref() == Some("ERA") {
        eonos_rt::println!("{}: this is an ERA app; open it from Era's Apps window", path);
        return Ok(());
    }
    let process = if app_type.as_deref() == Some("SHELL") {
        Process::spawn_sandboxed_with_stdio(path, &child_args, &env_values, None, Some(1))?
    } else {
        Process::spawn_with_stdio(path, &child_args, &env_values, None, Some(1))?
    };
    if background {
        reap_jobs(jobs);
        jobs.retain(|job| job.status.is_none());
        if jobs.len() >= MAX_JOBS {
            return Err(Error::System(eonos_rt::Error(11)));
        }
        let command = if background {
            alloc::format!("{} &", display.strip_suffix(" &").unwrap_or(display))
        } else {
            String::from(display)
        };
        eonos_rt::println!("[{}] started {}", process.pid(), command);
        jobs.push(Job {
            process,
            command,
            status: None,
        });
        return Ok(());
    }
    let status = process.wait()?;
    if status.code != 0 && status.code != 130 {
        eonos_rt::println!("exec: process {} exited {}", status.pid, status.code);
    }
    Ok(())
}

fn packaged_app_type(path: &str) -> Option<String> {
    let id = path.trim_end_matches('/').rsplit('/').next()?;
    if id.is_empty() || id.len() > 32
        || !id.bytes().all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_' || byte == b'-') {
        return None;
    }
    for manifest_path in [
        alloc::format!("/disk/apps/manifests/{}.eapp", id),
        alloc::format!("/apps/manifests/{}.eapp", id),
    ] {
        let Ok(mut file) = eonos_user::File::open(&manifest_path, eonos_user::flags::READ_ONLY) else { continue; };
        let Ok(manifest) = file.read_to_string() else { continue; };
        let mut lines = manifest.lines();
        if lines.next() != Some("EONOSAPP1") || lines.next() != Some(id) { continue; }
        let Some(kind) = lines.next() else { continue; };
        if kind == "SHELL" || kind == "ERA" { return Some(String::from(kind)); }
    }
    None
}

const MAX_EONAPP_BYTES: usize = 64 * 1024 * 1024 + 1044;

fn install_app_package(path: &str) -> Result<(), Error> {
    if !path.ends_with(".eonapp") { return Err(Error::Usage("install <package.eonapp>")); }
    let mut package_file = File::open(path, eonos_user::flags::READ_ONLY)?;
    let mut package = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        let count = package_file.read(&mut chunk)?;
        if count == 0 { break; }
        if package.len().checked_add(count).is_none_or(|size| size > MAX_EONAPP_BYTES) {
            return Err(Error::InvalidArgument);
        }
        package.extend_from_slice(&chunk[..count]);
    }
    if package.len() < 20 || &package[..8] != b"EONAPP1\0" { return Err(Error::InvalidArgument); }
    let metadata_len = u32::from_le_bytes(package[8..12].try_into().map_err(|_| Error::InvalidArgument)?) as usize;
    let elf_len = usize::try_from(u64::from_le_bytes(package[12..20].try_into().map_err(|_| Error::InvalidArgument)?))
        .map_err(|_| Error::InvalidArgument)?;
    let elf_offset = 20usize.checked_add(metadata_len).ok_or(Error::InvalidArgument)?;
    let end = elf_offset.checked_add(elf_len).ok_or(Error::InvalidArgument)?;
    if metadata_len == 0 || metadata_len > 1024 || elf_len == 0 || end != package.len() {
        return Err(Error::InvalidArgument);
    }
    let metadata = core::str::from_utf8(&package[20..elf_offset]).map_err(|_| Error::InvalidArgument)?;
    let mut fields = metadata.lines();
    if fields.next() != Some("EONOSAPP1") { return Err(Error::InvalidArgument); }
    let id = fields.next().ok_or(Error::InvalidArgument)?;
    let kind = fields.next().ok_or(Error::InvalidArgument)?;
    let name = fields.next().ok_or(Error::InvalidArgument)?;
    let description = fields.next().ok_or(Error::InvalidArgument)?;
    if fields.next().is_some() || !valid_app_id(id)
        || (kind != "SHELL" && kind != "ERA")
        || name.is_empty() || name.len() > 80 || description.is_empty() || description.len() > 160
        || name.bytes().chain(description.bytes()).any(|byte| byte < 0x20 || byte == 0x7f)
    { return Err(Error::InvalidArgument); }
    let filename = path.rsplit('/').next().unwrap_or(path);
    if filename != alloc::format!("{}.eonapp", id) { return Err(Error::InvalidArgument); }
    let elf = &package[elf_offset..end];
    let marker = if kind == "ERA" { b"EONOSAPP1:ERA".as_slice() } else { b"EONOSAPP1:SHELL".as_slice() };
    if elf.len() < 64 || &elf[..4] != b"\x7fELF" || !elf.windows(marker.len()).any(|part| part == marker) {
        return Err(Error::InvalidArgument);
    }

    ensure_directory("/disk/apps")?;
    ensure_directory("/disk/apps/bin")?;
    ensure_directory("/disk/apps/manifests")?;
    let installed_manifest = alloc::format!("/disk/apps/manifests/{}.eapp", id);
    if eonos_user::File::open(&installed_manifest, eonos_user::flags::READ_ONLY).is_ok() {
        eonos_rt::println!("install: '{}' is already installed; uninstall it first", id);
        return Ok(());
    }
    let executable = alloc::format!("/disk/apps/bin/{}", id);
    if eonos_user::File::open(&executable, eonos_user::flags::READ_ONLY).is_ok() {
        let _ = eonos_user::remove(&executable);
    }
    let mut program = File::open(
        &executable,
        eonos_user::flags::WRITE_ONLY | eonos_user::flags::CREATE | eonos_user::flags::TRUNCATE,
    )?;
    program.write_all(elf)?;
    program.close()?;
    let mut manifest = File::open(
        &installed_manifest,
        eonos_user::flags::WRITE_ONLY | eonos_user::flags::CREATE | eonos_user::flags::TRUNCATE,
    )?;
    manifest.write_all(metadata.as_bytes())?;
    manifest.close()?;
    eonos_rt::sync()?;
    eonos_rt::println!("Installed {} app '{}' ({})", kind, name, id);
    Ok(())
}

fn valid_app_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 32 && id.as_bytes()[0].is_ascii_lowercase()
        && id.bytes().all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_' || byte == b'-')
}

fn ensure_directory(path: &str) -> Result<(), Error> {
    match eonos_user::mkdir(path) {
        Ok(()) | Err(Error::System(eonos_rt::Error(17))) => Ok(()),
        Err(error) => Err(error),
    }
}

fn uninstall_app(id: &str) -> Result<(), Error> {
    if !valid_app_id(id) { return Err(Error::InvalidArgument); }
    let manifest = alloc::format!("/disk/apps/manifests/{}.eapp", id);
    let executable = alloc::format!("/disk/apps/bin/{}", id);
    eonos_user::remove(&manifest)?;
    eonos_user::remove(&executable)?;
    eonos_rt::sync()?;
    eonos_rt::println!("Uninstalled app '{}'", id);
    Ok(())
}

fn env_value<'a>(environment: &'a [String], name: &str) -> Option<&'a str> {
    environment.iter().find_map(|entry| {
        let (key, value) = entry.split_once('=')?;
        (key == name).then_some(value)
    })
}

fn set_env(environment: &mut Vec<String>, name: &str, value: &str) {
    let replacement = alloc::format!("{}={}", name, value);
    if let Some(entry) = environment.iter_mut().find(|entry| {
        entry.split_once('=').is_some_and(|(key, _)| key == name)
    }) {
        *entry = replacement;
    } else {
        environment.push(replacement);
    }
}

fn valid_env_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes.next().is_some_and(|first| first == b'_' || first.is_ascii_alphabetic())
        && bytes.all(|byte| byte == b'_' || byte.is_ascii_alphanumeric())
}

fn export_variables(args: &[String], environment: &mut Vec<String>) -> Result<(), Error> {
    if args.is_empty() {
        for entry in environment.iter() {
            eonos_rt::println!("export {}", entry);
        }
        return Ok(());
    }
    for assignment in args {
        let (name, value) = assignment.split_once('=').unwrap_or((assignment, ""));
        if !valid_env_name(name) {
            return Err(Error::InvalidArgument);
        }
        if assignment.contains('=') || env_value(environment, name).is_none() {
            set_env(environment, name, value);
        }
    }
    Ok(())
}

fn unset_variables(args: &[String], environment: &mut Vec<String>) -> Result<(), Error> {
    if args.is_empty() {
        return Err(Error::Usage("unset <name>..."));
    }
    for name in args {
        if !valid_env_name(name) {
            return Err(Error::InvalidArgument);
        }
        environment.retain(|entry| {
            !entry
                .split_once('=')
                .is_some_and(|(key, _)| key == name)
        });
    }
    Ok(())
}

fn find_command(command: &str, environment: &[String]) -> Option<String> {
    let path = env_value(environment, "PATH")?;
    for directory in path.split(':').map(|directory| if directory.is_empty() { "." } else { directory }) {
        if let Ok(entries) = eonos_user::list_directory(directory) {
            if entries.lines().any(|entry| entry == command) {
                return Some(if directory == "/" {
                    alloc::format!("/{}", command)
                } else {
                    alloc::format!("{}/{}", directory.trim_end_matches('/'), command)
                });
            }
        }
    }
    None
}

fn executable_path(command: &str, environment: &[String]) -> Option<String> {
    if command.contains('/') {
        Some(String::from(command))
    } else {
        find_command(command, environment)
    }
}

struct CommandSpec {
    words: Vec<String>,
    input: Option<String>,
    output: Option<(String, bool)>,
}

fn parse_pipeline(tokens: &[String]) -> Result<Vec<CommandSpec>, Error> {
    let mut commands = Vec::new();
    let mut current = CommandSpec {
        words: Vec::new(),
        input: None,
        output: None,
    };
    let mut index = 0;
    while index < tokens.len() {
        match tokens[index].as_str() {
            OP_PIPE => {
                if current.words.is_empty() {
                    return Err(Error::InvalidInput);
                }
                commands.push(current);
                current = CommandSpec {
                    words: Vec::new(),
                    input: None,
                    output: None,
                };
                index += 1;
            }
            OP_INPUT | OP_OUTPUT | OP_APPEND => {
                let operator = tokens[index].as_str();
                let path = tokens.get(index + 1).ok_or(Error::InvalidInput)?.clone();
                if operator == OP_INPUT {
                    if current.input.replace(path).is_some() {
                        return Err(Error::InvalidInput);
                    }
                } else if current.output.replace((path, operator == OP_APPEND)).is_some() {
                    return Err(Error::InvalidInput);
                }
                index += 2;
            }
            word => {
                current.words.push(String::from(word));
                index += 1;
            }
        }
    }
    if current.words.is_empty() {
        return Err(Error::InvalidInput);
    }
    commands.push(current);
    Ok(commands)
}

fn execute_pipeline(
    tokens: &[String],
    jobs: &mut Vec<Job>,
    environment: &[String],
) -> Result<(), Error> {
    if tokens.last().is_some_and(|word| word == "&") {
        return Err(Error::Usage("background pipelines are not supported yet"));
    }
    let commands = parse_pipeline(tokens)?;
    let mut pipes = Vec::new();
    for _ in 1..commands.len() {
        pipes.push(eonos_rt::pipe()?);
    }

    let mut redirection_files = Vec::new();
    let mut processes = Vec::new();
    let env_values: Vec<&str> = environment.iter().map(String::as_str).collect();
    let mut failure = None;
    let mut command_not_found = false;

    for (index, command) in commands.iter().enumerate() {
        let Some(name) = command.words.first() else {
            failure = Some(Error::InvalidInput);
            break;
        };
        let Some(path) = executable_path(name, environment) else {
            eonos_rt::println!("{}: command not found", name);
            failure = Some(Error::System(eonos_rt::Error(2)));
            command_not_found = true;
            break;
        };
        let input_fd = if let Some(path) = &command.input {
            match File::open(path, eonos_user::flags::READ_ONLY) {
                Ok(file) => {
                    let fd = file.fd();
                    redirection_files.push(file);
                    Some(fd)
                }
                Err(error) => {
                    failure = Some(error);
                    break;
                }
            }
        } else if index > 0 {
            Some(pipes[index - 1].0)
        } else {
            None
        };
        let output_fd = if let Some((path, append)) = &command.output {
            let flags = eonos_user::flags::WRITE_ONLY
                | eonos_user::flags::CREATE
                | if *append { eonos_user::flags::APPEND } else { eonos_user::flags::TRUNCATE };
            match File::open(path, flags) {
                Ok(file) => {
                    let fd = file.fd();
                    redirection_files.push(file);
                    Some(fd)
                }
                Err(error) => {
                    failure = Some(error);
                    break;
                }
            }
        } else if index + 1 < commands.len() {
            Some(pipes[index].1)
        } else {
            Some(1)
        };
        let arguments: Vec<&str> = command.words[1..].iter().map(String::as_str).collect();
        match Process::spawn_with_stdio(&path, &arguments, &env_values, input_fd, output_fd) {
            Ok(process) => processes.push(process),
            Err(error) => {
                failure = Some(error);
                break;
            }
        }
    }

    for (read_fd, write_fd) in pipes {
        let _ = eonos_rt::close(read_fd);
        let _ = eonos_rt::close(write_fd);
    }
    drop(redirection_files);

    if let Some(error) = failure {
        for process in &processes {
            let _ = process.signal(15);
        }
        for process in processes {
            let _ = process.wait();
        }
        if command_not_found {
            return Ok(());
        }
        return Err(error);
    }
    for process in processes {
        let status = process.wait()?;
        if status.code != 0 && status.code != 130 {
            eonos_rt::println!("process {} exited {}", status.pid, status.code);
        }
    }
    let _ = jobs;
    Ok(())
}

fn reap_jobs(jobs: &mut [Job]) {
    for job in jobs.iter_mut().filter(|job| job.status.is_none()) {
        if let Ok(Some(status)) = job.process.try_wait() {
            job.status = Some(status.code);
        }
    }
}

fn list_jobs(jobs: &mut [Job]) -> Result<(), Error> {
    reap_jobs(jobs);
    for job in jobs {
        match job.status {
            Some(status) => {
                eonos_rt::println!("[{}] done ({}) {}", job.process.pid(), status, job.command)
            }
            None => eonos_rt::println!("[{}] running {}", job.process.pid(), job.command),
        }
    }
    Ok(())
}

fn wait_jobs(args: &[String], jobs: &mut [Job]) -> Result<(), Error> {
    if args.is_empty() {
        for job in jobs.iter_mut().filter(|job| job.status.is_none()) {
            let status = job.process.wait()?;
            job.status = Some(status.code);
        }
        return Ok(());
    }
    let pid = required(args, 0, "wait [pid]")?
        .parse::<usize>()
        .map_err(|_| Error::InvalidArgument)?;
    if args.len() > 1 {
        return Err(Error::Usage("wait [pid]"));
    }
    let job = jobs
        .iter_mut()
        .find(|job| job.process.pid() == pid)
        .ok_or(Error::Usage("wait [known pid]"))?;
    if job.status.is_none() {
        job.status = Some(job.process.wait()?.code);
    }
    Ok(())
}

fn kill_job(args: &[String], jobs: &mut [Job]) -> Result<(), Error> {
    let pid = required(args, 0, "kill <pid>")?
        .parse::<usize>()
        .map_err(|_| Error::InvalidArgument)?;
    exact_args(args, 1, "kill <pid>")?;
    let job = jobs
        .iter_mut()
        .find(|job| job.process.pid() == pid)
        .ok_or(Error::Usage("kill <job pid>"))?;
    job.process.signal(15)?;
    if let Some(status) = job.process.try_wait()? {
        job.status = Some(status.code);
    }
    eonos_rt::println!("kill: sent SIGTERM to {}", pid);
    Ok(())
}

fn write_file(args: &[String]) -> Result<(), Error> {
    let path = required(args, 0, "write <file> <text>")?;
    let contents = args[1..].join(" ");
    let mut file = File::open(
        path,
        eonos_user::flags::WRITE_ONLY | eonos_user::flags::CREATE | eonos_user::flags::TRUNCATE,
    )?;
    file.write_all(contents.as_bytes())?;
    eonos_rt::println!("write: saved");
    Ok(())
}

fn touch(path: &str) -> Result<(), Error> {
    let file = File::open(
        path,
        eonos_user::flags::WRITE_ONLY | eonos_user::flags::CREATE,
    )?;
    file.close()?;
    eonos_rt::println!("touch: created {}", path);
    Ok(())
}

fn copy_file(args: &[String]) -> Result<(), Error> {
    exact_args(args, 2, "cp <source> <destination>")?;
    let source = required(args, 0, "cp <source> <destination>")?;
    let destination = required(args, 1, "cp <source> <destination>")?;
    let mut input = File::open(source, eonos_user::flags::READ_ONLY)?;
    let data = input.read_to_end()?;
    let mut output = File::open(
        destination,
        eonos_user::flags::WRITE_ONLY | eonos_user::flags::CREATE | eonos_user::flags::TRUNCATE,
    )?;
    output.write_all(&data)?;
    eonos_rt::println!("cp: copied {} bytes", data.len());
    Ok(())
}

fn edit_file(path: &str) -> Result<(), Error> {
    eonos_rt::println!("Rust editor -- append lines, .save writes, .quit exits");
    let mut added = String::new();
    loop {
        let _ = eonos_user::print("edit> ");
        let Some(line) = eonos_user::read_line(0)? else {
            return Ok(());
        };
        match line.as_str() {
            ".quit" => return Ok(()),
            ".save" => break,
            _ => {
                added.push_str(&line);
                added.push('\n');
            }
        }
    }

    let mut contents = match File::open(path, eonos_user::flags::READ_ONLY) {
        Ok(mut existing) => existing.read_to_string()?,
        Err(Error::System(eonos_rt::Error(2))) => String::new(),
        Err(error) => return Err(error),
    };
    contents.push_str(&added);
    let mut output = File::open(
        path,
        eonos_user::flags::WRITE_ONLY | eonos_user::flags::CREATE | eonos_user::flags::TRUNCATE,
    )?;
    output.write_all(contents.as_bytes())?;
    eonos_rt::println!("[ saved ]");
    Ok(())
}

fn mount_device(args: &[String]) -> Result<(), Error> {
    let device = required(args, 0, "mount <device> <directory>")?;
    let directory = required(args, 1, "mount <device> <directory>")?;
    eonos_user::mount(device, directory)?;
    eonos_rt::println!("mount: mounted");
    Ok(())
}

eonos_rt::entry!(main);
