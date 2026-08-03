void *memcpy(void *dest, const void *src, unsigned long n) {
    unsigned char *d = dest;
    const unsigned char *s = src;
    while (n--) *d++ = *s++;
    return dest;
}

int bcmp(const void *s1, const void *s2, unsigned long n) {
    const unsigned char *a = s1;
    const unsigned char *b = s2;
    while (n--) {
        if (*a != *b) return 1;
        a++;
        b++;
    }
    return 0;
}

void __aeabi_memcpy(void *dest, const void *src, unsigned long n) {
    memcpy(dest, src, n);
}
