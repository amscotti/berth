//! Copy-run command tests: ports, quoting and redaction live here so the
//! implementation stays within the 600-line rule.
const std = @import("std");
const lifetime = @import("lifetime.zig");

const fromInspect = lifetime.fromInspect;

test "the run command reproduces ports, env and mounts, and skips the noise" {
    const doc =
        \\{"Name":"/shop-web-1","Config":{"Image":"nginx:alpine","Env":["PATH=/usr/bin","TOKEN=abc def","MODE=prod","APOSTROPHE=it's"]},
        \\ "HostConfig":{"PortBindings":{"80/tcp":[{"HostIp":"","HostPort":"8080"},{"HostIp":"127.0.0.1","HostPort":"8081"}],
        \\                                "53/udp":[{"HostIp":"","HostPort":"1053"}],
        \\                                "53/sctp":[{"HostIp":"","HostPort":"1054"}],
        \\                                "9000/tcp":[{"HostIp":"","HostPort":""}]}},
        \\ "Mounts":[{"Type":"bind","Source":"/Users/x/site","Destination":"/usr/share/nginx/html","RW":false},
        \\            {"Type":"volume","Name":"shop-data","Destination":"/data"},
        \\            {"Type":"bind","Source":"/srv/a:b","Destination":"/wrong"},
        \\            {"Type":"tmpfs","Destination":"/tmp"}],
        \\ "RestartCount":1,"State":{"StartedAt":"2026-09-22T18:13:20Z"}}
    ;
    const life = fromInspect(std.testing.allocator, doc);
    const cmd = life.run_cmd.?;
    defer std.testing.allocator.free(cmd);
    try std.testing.expectEqualStrings(
        "docker run -d --name shop-web-1 -p 8080:80 -p 127.0.0.1:8081:80 -p 1053:53/udp -p 1054:53/sctp -e 'TOKEN=<redacted>' -e MODE=prod -e 'APOSTROPHE=it'\\''s' -v /Users/x/site:/usr/share/nginx/html:ro -v shop-data:/data nginx:alpine",
        cmd,
    );
    try std.testing.expect(life.run_redacted);
    try std.testing.expectEqual(@as(u32, 1), life.restarts);
}

test "a bare container copies as one line with no empty flags" {
    const doc =
        \\{"Name":"/solo","Config":{"Image":"alpine"},"RestartCount":0,"State":{"StartedAt":""}}
    ;
    const life = fromInspect(std.testing.allocator, doc);
    defer if (life.run_cmd) |cmd| std.testing.allocator.free(cmd);
    try std.testing.expectEqualStrings("docker run -d --name solo alpine", life.run_cmd.?);
    try std.testing.expect(!life.run_redacted);
    try std.testing.expect(life.started_at == null);
}

test "hostile port fields are dropped instead of reaching the shell" {
    const doc =
        \\{"Name":"/hostile","Config":{"Image":"alpine"},
        \\ "HostConfig":{"PortBindings":{
        \\   "80/tcp":[{"HostIp":"127.0.0.1; touch /tmp/pwn","HostPort":"8080"}],
        \\   "81/tcp":[{"HostIp":"","HostPort":"8080; id"}],
        \\   "82/tcp":[{"HostIp":"$(id)","HostPort":"8082"}],
        \\   "83/tcp;curl evil":[{"HostIp":"","HostPort":"8083"}],
        \\   "84/tcp":[{"HostIp":"","HostPort":"70000"}],
        \\   "85/tcp":[{"HostIp":"","HostPort":"-1"}],
        \\   "86/tcp":[{"HostIp":"","HostPort":"0"}],
        \\   "87/tcp":[{"HostIp":"","HostPort":"8087"}],
        \\   "88/tcp":[{"HostIp":7,"HostPort":"8088"}]}},
        \\ "State":{"StartedAt":""}}
    ;
    const life = fromInspect(std.testing.allocator, doc);
    defer if (life.run_cmd) |cmd| std.testing.allocator.free(cmd);
    const cmd = life.run_cmd.?;
    // Only the one well-formed binding survived; nothing else can split the
    // command or start a second one.
    try std.testing.expectEqualStrings("docker run -d --name hostile -p 8087:87 alpine", cmd);
    try std.testing.expect(std.mem.indexOfScalar(u8, cmd, ';') == null);
    try std.testing.expect(std.mem.indexOfScalar(u8, cmd, '$') == null);
    try std.testing.expect(std.mem.indexOfScalar(u8, cmd, '`') == null);
    try std.testing.expect(std.mem.indexOf(u8, cmd, "touch") == null);
    try std.testing.expect(std.mem.indexOf(u8, cmd, "curl") == null);
}

test "a valid IPv6 binding stays a single quoted -p word" {
    const doc =
        \\{"Name":"/v6","Config":{"Image":"alpine"},
        \\ "HostConfig":{"PortBindings":{
        \\   "80/tcp":[{"HostIp":"::1","HostPort":"8080"}],
        \\   "53/udp":[{"HostIp":"0.0.0.0","HostPort":"1053"}]}},
        \\ "State":{"StartedAt":""}}
    ;
    const life = fromInspect(std.testing.allocator, doc);
    defer if (life.run_cmd) |cmd| std.testing.allocator.free(cmd);
    try std.testing.expectEqualStrings(
        "docker run -d --name v6 -p '[::1]:8080:80' -p 1053:53/udp alpine",
        life.run_cmd.?,
    );
}

test "secret environment values are redacted, ordinary ones survive" {
    const doc =
        \\{"Name":"/secrets","Config":{"Image":"app","Env":[
        \\   "GITHUB_TOKEN=ghp_live_value","db_password=hunter2","API_KEY=abc123",
        \\   "MY_SECRET=shh","AUTH_CODE=authcode","MODE=prod","GREETING=hello world",
        \\   "PASSCODE=passcode"]},
        \\ "State":{"StartedAt":""}}
    ;
    const life = fromInspect(std.testing.allocator, doc);
    defer if (life.run_cmd) |cmd| std.testing.allocator.free(cmd);
    const cmd = life.run_cmd.?;
    try std.testing.expect(life.run_redacted);
    inline for ([_][]const u8{ "ghp_live_value", "hunter2", "abc123", "shh", "authcode", "passcode" }) |secret| {
        try std.testing.expect(std.mem.indexOf(u8, cmd, secret) == null);
    }
    try std.testing.expect(std.mem.indexOf(u8, cmd, "'GITHUB_TOKEN=<redacted>'") != null);
    try std.testing.expect(std.mem.indexOf(u8, cmd, "'db_password=<redacted>'") != null);
    try std.testing.expect(std.mem.indexOf(u8, cmd, "'API_KEY=<redacted>'") != null);
    try std.testing.expect(std.mem.indexOf(u8, cmd, "'AUTH_CODE=<redacted>'") != null);
    try std.testing.expect(std.mem.indexOf(u8, cmd, "'PASSCODE=<redacted>'") != null);
    try std.testing.expect(std.mem.indexOf(u8, cmd, "MODE=prod") != null);
    try std.testing.expect(std.mem.indexOf(u8, cmd, "'GREETING=hello world'") != null);
}

test "URLs and common credential keys are redacted too" {
    const doc =
        \\{"Name":"/urls","Config":{"Image":"app","Env":[
        \\   "DATABASE_URL=postgres://user:dbpass@db/app",
        \\   "REPORTING_DSN=postgres://user:dsnpass@db/report",
        \\   "SERVICE_URI=https://user:uripass@example/x",
        \\   "DOCKER_AUTH_CONFIG={\"auths\":{\"r\":{\"auth\":\"authpass\"}}}",
        \\   "PRIVATE_VALUE=privpass","LOGIN_PWD=pwdpass","SK=skpass",
        \\   "GREETING=https://user:urlpass@example/path","MODE=prod"]},
        \\ "State":{"StartedAt":""}}
    ;
    const life = fromInspect(std.testing.allocator, doc);
    defer if (life.run_cmd) |cmd| std.testing.allocator.free(cmd);
    const cmd = life.run_cmd.?;
    try std.testing.expect(life.run_redacted);
    inline for ([_][]const u8{ "dbpass", "dsnpass", "uripass", "authpass", "privpass", "pwdpass", "skpass", "urlpass" }) |secret| {
        try std.testing.expect(std.mem.indexOf(u8, cmd, secret) == null);
    }
    inline for ([_][]const u8{ "DATABASE_URL", "REPORTING_DSN", "SERVICE_URI", "DOCKER_AUTH_CONFIG", "PRIVATE_VALUE", "LOGIN_PWD", "SK", "GREETING" }) |key| {
        const at = std.mem.indexOf(u8, cmd, key).?;
        try std.testing.expect(std.mem.startsWith(u8, cmd[at + key.len ..], "=<redacted>'"));
    }
    try std.testing.expect(std.mem.indexOf(u8, cmd, "MODE=prod") != null);
}

test "session credentials redact while token lookalikes survive" {
    const doc =
        \\{"Name":"/tokens","Config":{"Image":"app","Env":[
        \\   "SESSION_ID=sess123","COOKIE=coo123","JWT=jwt123",
        \\   "BEARER=bea123","SIGNATURE=sig123","AWS_SECRET_ACCESS_KEY=aws123",
        \\   "MONKEY=banana","TASK_ID=42","DESKTOP=gnome","RISK=low",
        \\   "CURL_CA_BUNDLE=/etc/ca","AUTHOR=alice","SECURITY_LEVEL=high"]},
        \\ "State":{"StartedAt":""}}
    ;
    const life = fromInspect(std.testing.allocator, doc);
    defer if (life.run_cmd) |cmd| std.testing.allocator.free(cmd);
    const cmd = life.run_cmd.?;
    try std.testing.expect(life.run_redacted);
    inline for ([_][]const u8{ "sess123", "coo123", "jwt123", "bea123", "sig123", "aws123" }) |secret| {
        try std.testing.expect(std.mem.indexOf(u8, cmd, secret) == null);
    }
    inline for ([_][]const u8{ "MONKEY=banana", "TASK_ID=42", "DESKTOP=gnome", "RISK=low", "CURL_CA_BUNDLE=/etc/ca", "AUTHOR=alice", "SECURITY_LEVEL=high" }) |pair| {
        try std.testing.expect(std.mem.indexOf(u8, cmd, pair) != null);
    }
}

test "plural compounds and camelCase credential keys are redacted" {
    const doc =
        \\{"Name":"/forms","Config":{"Image":"app","Env":[
        \\   "PASSCODE=pc1","PASSPHRASE=pp1","PASSWD=pw1","SECRETS=sec1",
        \\   "API_KEYS=ak1","CREDENTIALS=cred1","PASSWORDS=pwds1","COOKIES=coo1",
        \\   "apiKey=ak2","accessToken=at2","sessionId=sid2","MODE=prod"]},
        \\ "State":{"StartedAt":""}}
    ;
    const life = fromInspect(std.testing.allocator, doc);
    defer if (life.run_cmd) |cmd| std.testing.allocator.free(cmd);
    const cmd = life.run_cmd.?;
    try std.testing.expect(life.run_redacted);
    inline for ([_][]const u8{ "pc1", "pp1", "pw1", "sec1", "ak1", "cred1", "pwds1", "coo1", "ak2", "at2", "sid2" }) |secret| {
        try std.testing.expect(std.mem.indexOf(u8, cmd, secret) == null);
    }
    inline for ([_][]const u8{ "PASSCODE", "PASSPHRASE", "PASSWD", "SECRETS", "API_KEYS", "CREDENTIALS", "PASSWORDS", "COOKIES", "apiKey", "accessToken", "sessionId" }) |key| {
        const at = std.mem.indexOf(u8, cmd, key).?;
        try std.testing.expect(std.mem.startsWith(u8, cmd[at + key.len ..], "=<redacted>'"));
    }
    try std.testing.expect(std.mem.indexOf(u8, cmd, "MODE=prod") != null);
}

test "an empty environment value keeps its argument slot" {
    const doc =
        \\{"Name":"/empties","Config":{"Image":"alpine","Env":["","MODE=prod"]},"State":{"StartedAt":""}}
    ;
    const life = fromInspect(std.testing.allocator, doc);
    defer if (life.run_cmd) |cmd| std.testing.allocator.free(cmd);
    try std.testing.expectEqualStrings(
        "docker run -d --name empties -e '' -e MODE=prod alpine",
        life.run_cmd.?,
    );
}

test "a valid inspect with deep PortBindings is ignored before parsing" {
    const gpa = std.testing.allocator;
    var text: std.ArrayList(u8) = .empty;
    defer text.deinit(gpa);
    try text.appendSlice(gpa, "{\"Name\":\"/deep\",\"Config\":{\"Image\":\"alpine\"},\"HostConfig\":{\"PortBindings\":{\"80/tcp\":[{\"HostIp\":\"\",\"HostPort\":\"8080\",\"Nested\":");
    for (0..200) |_| try text.append(gpa, '[');
    try text.append(gpa, '0');
    for (0..200) |_| try text.append(gpa, ']');
    try text.appendSlice(gpa, "}]}},\"State\":{\"StartedAt\":\"\"}}");

    // The document is valid and would parse without the guard; the depth is
    // the only reason it is refused.
    var parsed = try std.json.parseFromSlice(std.json.Value, gpa, text.items, .{});
    defer parsed.deinit();
    try std.testing.expect(parsed.value == .object);

    const life = fromInspect(gpa, text.items);
    try std.testing.expect(life.run_cmd == null);
    try std.testing.expect(!life.run_redacted);
}
