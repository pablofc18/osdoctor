/*
 * output.h - report renderers.
 */
#ifndef OSDOCTOR_OUTPUT_H
#define OSDOCTOR_OUTPUT_H

#include "osdoctor.h"

/* Human-readable, grouped report. Colorized when stdout is a TTY and
 * NO_COLOR is unset. */
void output_text(const check_result_list_t *results);

/* Machine-readable JSON report (pretty-printed, 2-space indent). */
void output_json(const check_result_list_t *results);

#endif /* OSDOCTOR_OUTPUT_H */
