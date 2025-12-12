#include "include/log.hpp"

#include <mapnik/debug.hpp>

#include <iostream>

std::ostream* log_out = &std::cerr;

void set_logging(std::ostream* os) {
  log_out = os;
}

static std::streambuf* old_buf;

void clog_redirect() {
  old_buf = std::clog.rdbuf(log_out->rdbuf());
}

void restore_clog() {
  std::clog.rdbuf(old_buf);
}
