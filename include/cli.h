/*
 * cli.h - argument parsing and command dispatch.
 */
#ifndef OSDOCTOR_CLI_H
#define OSDOCTOR_CLI_H

/* Parse argv, run the requested command, render output, and return the
 * process exit code (0/1/2 from results, 3 for usage/internal errors). */
int cli_main(int argc, char **argv);

#endif /* OSDOCTOR_CLI_H */
