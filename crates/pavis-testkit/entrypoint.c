#include <stdlib.h>
#include <stdio.h>
#include <unistd.h>

int main(int argc, char **argv) {
    const char *name = getenv("BINARY_NAME");
    if (!name || !name[0]) {
        fprintf(stderr, "BINARY_NAME is not set\n");
        return 127;
    }

    char path[512];
    snprintf(path, sizeof(path), "/usr/local/bin/%s", name);

    // argv[0] should be the program name; keep args as-is
    argv[0] = (char*)name;
    execv(path, argv);

    perror("execv failed");
    return 127;
}