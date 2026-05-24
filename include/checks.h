/*
 * checks.h - check group runners.
 *
 * Each runner appends one or more results to `results` and returns 0 on
 * success or -1 on an internal error (e.g. allocation failure). A returned
 * -1 is a tooling error and maps to exit code 3; FAIL/WARN checks are NOT
 * errors and still return 0.
 */
#ifndef OSDOCTOR_CHECKS_H
#define OSDOCTOR_CHECKS_H

#include "osdoctor.h"

int run_system_checks(check_result_list_t *results);
int run_service_checks(check_result_list_t *results);
int run_package_checks(check_result_list_t *results);
int run_desktop_checks(check_result_list_t *results);
int run_all_checks(check_result_list_t *results);

#endif /* OSDOCTOR_CHECKS_H */
