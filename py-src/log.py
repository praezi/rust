#!/usr/bin/env python
# Logging configuration
#
# (c) 2017 - onwards Georgios Gousios <gousiosg@gmail.com>
#
# MIT/APACHE licensed -- check LICENSE files in top dir

import sys
import logging

logging.basicConfig(
    format="%(asctime)s [%(process)d]%(filename)s:%(lineno)d(%(funcName)s) --- %(message)s",
    level=logging.DEBUG,
    stream=sys.stderr,
)

from logging import debug, error, info
