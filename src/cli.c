/*
 * cli.c - argument parsing and command dispatch.
 */
#include "cli.h"
#include "checks.h"
#include "osdoctor.h"
#include "output.h"

#include <stdio.h>
#include <string.h>

typedef enum {
    CMD_NONE,
    CMD_SCAN,
    CMD_SYSTEM,
    CMD_SERVICES,
    CMD_PACKAGES,
    CMD_DESKTOP,
    CMD_VERSION,
    CMD_HELP
} command_t;

static void print_usage(FILE *out) {
    fprintf(out,
            "osdoctor %s - Linux desktop health checks for Arch/Hyprland systems\n"
            "\n"
            "Usage:\n"
            "  osdoctor [command] [options]\n"
            "\n"
            "Commands:\n"
            "  scan        Run all health checks (default when no command is given)\n"
            "  system      Run system checks only\n"
            "  services    Run service checks only\n"
            "  packages    Run package checks only\n"
            "  desktop     Run desktop/session checks only\n"
            "  version     Print version and exit\n"
            "  help        Show this help and exit\n"
            "\n"
            "Options:\n"
            "  --json          Output results as JSON\n"
            "  --strict        Treat warnings as failures for the exit code\n"
            "  -h, --help      Show this help\n"
            "  -V, --version   Print version\n"
            "\n"
            "Exit codes:\n"
            "  0  all checks passed\n"
            "  1  warnings found, no failures\n"
            "  2  one or more failures found\n"
            "  3  invalid usage or internal error\n",
            OSDOCTOR_VERSION);
}

static command_t parse_command(const char *arg) {
    if (strcmp(arg, "scan") == 0) {
        return CMD_SCAN;
    }
    if (strcmp(arg, "system") == 0) {
        return CMD_SYSTEM;
    }
    if (strcmp(arg, "services") == 0) {
        return CMD_SERVICES;
    }
    if (strcmp(arg, "packages") == 0) {
        return CMD_PACKAGES;
    }
    if (strcmp(arg, "desktop") == 0) {
        return CMD_DESKTOP;
    }
    if (strcmp(arg, "version") == 0) {
        return CMD_VERSION;
    }
    if (strcmp(arg, "help") == 0) {
        return CMD_HELP;
    }
    return CMD_NONE;
}

static int run_command(command_t cmd, check_result_list_t *results) {
    switch (cmd) {
    case CMD_SYSTEM:
        return run_system_checks(results);
    case CMD_SERVICES:
        return run_service_checks(results);
    case CMD_PACKAGES:
        return run_package_checks(results);
    case CMD_DESKTOP:
        return run_desktop_checks(results);
    case CMD_SCAN:
    default:
        return run_all_checks(results);
    }
}

int cli_main(int argc, char **argv) {
    bool json = false;
    bool strict = false;
    bool want_help = false;
    bool want_version = false;
    command_t cmd = CMD_NONE;

    for (int i = 1; i < argc; i++) {
        const char *arg = argv[i];
        if (strcmp(arg, "--json") == 0) {
            json = true;
        } else if (strcmp(arg, "--strict") == 0) {
            strict = true;
        } else if (strcmp(arg, "-h") == 0 || strcmp(arg, "--help") == 0) {
            want_help = true;
        } else if (strcmp(arg, "-V") == 0 || strcmp(arg, "--version") == 0) {
            want_version = true;
        } else if (arg[0] == '-') {
            fprintf(stderr, "osdoctor: unknown option '%s'\n\n", arg);
            print_usage(stderr);
            return 3;
        } else if (cmd != CMD_NONE) {
            fprintf(stderr, "osdoctor: unexpected argument '%s'\n", arg);
            return 3;
        } else {
            cmd = parse_command(arg);
            if (cmd == CMD_NONE) {
                fprintf(stderr, "osdoctor: unknown command '%s'\n\n", arg);
                print_usage(stderr);
                return 3;
            }
        }
    }

    if (want_help || cmd == CMD_HELP) {
        print_usage(stdout);
        return 0;
    }
    if (want_version || cmd == CMD_VERSION) {
        printf("osdoctor %s\n", OSDOCTOR_VERSION);
        return 0;
    }
    if (cmd == CMD_NONE) {
        /* default action, no command given: print_usage */
        fprintf(stderr, "osdoctor: no command given\n\n");
        print_usage(stderr);
        return 3;
    }

    check_result_list_t results;
    results_init(&results);

    if (run_command(cmd, &results) != 0) {
        fprintf(stderr, "osdoctor: internal error while running checks\n");
        results_free(&results);
        return 3;
    }

    if (json) {
        output_json(&results);
    } else {
        output_text(&results);
    }

    check_summary_t summary = summarize_results(&results);
    int code = exit_code_from_summary(summary, strict);
    results_free(&results);
    return code;
}
