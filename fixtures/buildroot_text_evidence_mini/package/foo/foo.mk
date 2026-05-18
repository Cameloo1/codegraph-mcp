################################################################################
#
# foo
#
################################################################################

FOO_VERSION = 1.2.3
FOO_SITE = https://example.com/foo
FOO_LICENSE = MIT
FOO_LICENSE_FILES = COPYING
FOO_DEPENDENCIES = bar host-baz

$(eval $(generic-package))
