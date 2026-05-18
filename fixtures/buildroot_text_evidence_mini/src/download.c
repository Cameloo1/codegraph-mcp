#include <stdio.h>

int download_archive(const char *url)
{
    if (url == NULL) {
        return -1;
    }

    printf("download %s\n", url);
    return 0;
}
