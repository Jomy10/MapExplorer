#pragma once

#include <ostream>

extern std::ostream* log_out;

#define INFO (assert(log_out != nullptr));*log_out

void set_logging(std::ostream* os);

void clog_redirect();
void restore_clog();
