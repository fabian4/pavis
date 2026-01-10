#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <errno.h>
#include <netdb.h>
#include <sys/socket.h>
#include <netinet/in.h>

#define MAX_BUFFER 4096

static int run_healthcheck(const char *url) {
    char host[256] = {0};
    char path[1024] = {0};
    char port_str[16] = "80";
    int port = 80;

    if (strncmp(url, "http://", 7) != 0) {
        fprintf(stderr, "Error: Only http:// is supported\n");
        return 1;
    }

    const char *host_start = url + 7;
    const char *path_start = strchr(host_start, '/');
    if (path_start) {
        snprintf(path, sizeof(path), "%s", path_start);
        size_t host_len = path_start - host_start;
        if (host_len >= sizeof(host)) host_len = sizeof(host) - 1;
        strncpy(host, host_start, host_len);
    } else {
        snprintf(path, sizeof(path), "/");
        strncpy(host, host_start, sizeof(host) - 1);
    }

    char *colon = strchr(host, ':');
    if (colon) {
        *colon = '\0';
        snprintf(port_str, sizeof(port_str), "%s", colon + 1);
        port = atoi(port_str);
    }

    struct addrinfo hints, *res;
    memset(&hints, 0, sizeof(hints));
    hints.ai_family = AF_UNSPEC;
    hints.ai_socktype = SOCK_STREAM;

    if (getaddrinfo(host, port_str, &hints, &res) != 0) {
        fprintf(stderr, "Error: Failed to resolve host %s\n", host);
        return 1;
    }

    int sock = socket(res->ai_family, res->ai_socktype, res->ai_protocol);
    if (sock < 0) {
        perror("Error: socket");
        freeaddrinfo(res);
        return 1;
    }

    struct timeval timeout;
    timeout.tv_sec = 5;
    timeout.tv_usec = 0;
    setsockopt(sock, SOL_SOCKET, SO_RCVTIMEO, &timeout, sizeof(timeout));
    setsockopt(sock, SOL_SOCKET, SO_SNDTIMEO, &timeout, sizeof(timeout));

    if (connect(sock, res->ai_addr, res->ai_addrlen) < 0) {
        perror("Error: connect");
        close(sock);
        freeaddrinfo(res);
        return 1;
    }
    freeaddrinfo(res);

    char request[MAX_BUFFER];
    int req_len = snprintf(request, sizeof(request),
        "GET %s HTTP/1.1\r\n"
        "Host: %s\r\n"
        "User-Agent: Pavis-Healthcheck/1.0\r\n"
        "Connection: close\r\n"
        "\r\n", path, host);

    if (send(sock, request, req_len, 0) < 0) {
        perror("Error: send");
        close(sock);
        return 1;
    }

    char response[MAX_BUFFER];
    ssize_t received = recv(sock, response, sizeof(response) - 1, 0);
    close(sock);

    if (received < 0) {
        perror("Error: recv");
        return 1;
    }
    response[received] = '\0';

    int status_code = 0;
    if (sscanf(response, "HTTP/%%*d.%%*d %%d", &status_code) != 1) {
        fprintf(stderr, "Error: Invalid HTTP response\n");
        return 1;
    }

    if (status_code >= 200 && status_code < 400) {
        return 0;
    } else {
        fprintf(stderr, "Unhealthy: HTTP %d\n", status_code);
        return 1;
    }
}

int main(int argc, char *argv[]) {
    if (argc >= 3 && strcmp(argv[1], "healthcheck") == 0) {
        return run_healthcheck(argv[2]);
    }

    char *binary_name = getenv("BINARY_NAME");
    if (!binary_name) {
        fprintf(stderr, "Error: BINARY_NAME environment variable not set\n");
        return 1;
    }

    char binary_path[256];
    snprintf(binary_path, sizeof(binary_path), "/usr/local/bin/%s", binary_name);

    execv(binary_path, argv);

    fprintf(stderr, "Error: Failed to exec %s: %s\n", binary_path, strerror(errno));
    return 1;
}
