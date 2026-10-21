/* SPDX-License-Identifier: MPL-2.0
 *
 * Golden-vector generator for src/e2e.rs parity tests (issue #185;
 * testdata/c_rcp_e2e_golden.txt). Calls c-RCP's real rcp_acf_encode_abb/
 * _gbb, rcp_e2e_wrap_framed/_unwrap_framed, rcp_e2e_compute_crc and
 * rcp_e2e_compute_fragmented_crc (generated against c-RCP a4f3a1f).
 * Regenerate with:
 *
 *   cmake -S <c-RCP> -B /tmp/crcp-build && cmake --build /tmp/crcp-build
 *   cc -std=c99 -I<c-RCP>/include scripts/c-rcp-golden/e2e_gen.c \
 *      /tmp/crcp-build/librcp.a -o /tmp/e2e_gen
 *   /tmp/e2e_gen > testdata/c_rcp_e2e_golden.txt
 *
 * src/e2e.rs embeds these values as consts and checks this file has not
 * drifted from them (embedded_golden_vectors_match_the_c_rcp_harness_output).
 */
#include <rcp/acf.h>
#include <rcp/e2e.h>
#include <stdio.h>
#include <string.h>
#include <stdlib.h>

static void hex(const char *name, const uint8_t *p, size_t n) {
    size_t i; printf("%s (%zu): ", name, n);
    for (i = 0; i < n; i++) printf("%02x", p[i]);
    printf("\n");
}

int main(void) {
    /* Vector 1: NTSCF + ACF_ABB */
    {
        rcp_acf_byte_message_info_t h; memset(&h, 0, sizeof h);
        const uint8_t pl[6] = {0xA1,0xA2,0xA3,0xA4,0xA5,0xA6};
        h.byte_bus_id = 0x123; h.transaction_num = 0x5A; h.op = RCP_ACF_OP_WRITE;
        h.hs = 1; h.evt = 0;
        rcp_bytes_t plain = rcp_acf_encode_abb(&h, pl, sizeof pl);
        hex("v1 plain", plain.data, plain.len);
        /* tu=true / ts nonzero deliberately: _framed must force them for NTSCF */
        rcp_bytes_t w = rcp_e2e_wrap_framed(0x0011223344556677ULL, true, 0x80, true, 0xDEADBEEFu, plain.data, plain.len);
        hex("v1 wrapped", w.data, w.len);
        rcp_bytes_t back; int rc = rcp_e2e_unwrap_framed(0x0011223344556677ULL, true, 0x80, false, 0, w.data, w.len, &back);
        printf("v1 unwrap rc=%d eq=%d\n", rc, back.len == plain.len && !memcmp(back.data, plain.data, plain.len));
    }
    /* Vector 2: TSCF + ACF_GBB */
    {
        rcp_acf_gbb_header_t g; memset(&g, 0, sizeof g);
        const uint8_t pl[5] = {0xB1,0xB2,0xB3,0xB4,0xB5};
        g.info.byte_bus_id = 0x7FF; g.info.transaction_num = 0x33; g.info.op = RCP_ACF_OP_READ;
        g.info.read_size_or_segment_num = 0x004; g.info.mtv = RCP_ACF_MTV_VALID; g.info.cs = 1;
        g.message_timestamp = 0x0102030405060708ULL;
        rcp_bytes_t plain = rcp_acf_encode_gbb(&g, pl, sizeof pl);
        hex("v2 plain", plain.data, plain.len);
        /* TSCF octet1: sv=1 version=0 mr=1 tv=1 -> 0x89 */
        rcp_bytes_t w = rcp_e2e_wrap_framed(0x8899AABBCCDDEEFFULL, false, 0x89, true, 0xCAFEBABEu, plain.data, plain.len);
        hex("v2 wrapped", w.data, w.len);
        rcp_bytes_t back; int rc = rcp_e2e_unwrap_framed(0x8899AABBCCDDEEFFULL, false, 0x89, true, 0xCAFEBABEu, w.data, w.len, &back);
        printf("v2 unwrap rc=%d eq=%d\n", rc, back.len == plain.len && !memcmp(back.data, plain.data, plain.len));
        rc = rcp_e2e_unwrap_framed(0x8899AABBCCDDEEFFULL, false, 0x89, false, 0xCAFEBABEu, w.data, w.len, &back);
        printf("v2 unwrap wrong tu rc=%d\n", rc);
    }
    /* Vector 3: fragment train, NTSCF, ACF_ABB, 13-byte payload split 5/5/3 */
    {
        const uint8_t pl[13] = {0xC0,0xC1,0xC2,0xC3,0xC4,0xC5,0xC6,0xC7,0xC8,0xC9,0xCA,0xCB,0xCC};
        rcp_bytes_t f[3];
        int i;
        for (i = 0; i < 3; i++) {
            rcp_acf_byte_message_info_t h; memset(&h, 0, sizeof h);
            h.byte_bus_id = 0x042; h.transaction_num = 0x77; h.op = RCP_ACF_OP_WRITE;
            h.ms = (i < 2) ? 1 : 0;
            h.read_size_or_segment_num = (i < 2) ? (uint16_t)i : 0;
            f[i] = rcp_acf_encode_abb(&h, pl + 5 * i, i < 2 ? 5 : 3);
        }
        hex("v3 frag0 plain", f[0].data, f[0].len);
        hex("v3 frag1 plain", f[1].data, f[1].len);
        hex("v3 frag2 plain", f[2].data, f[2].len);
        uint32_t crc = rcp_e2e_compute_fragmented_crc(0x82, 0x80, false, 0x0102030405060708ULL, 0,
                                                       f[0].data, RCP_ACF_ABB_HEADER_LEN, pl, sizeof pl);
        printf("v3 fragmented crc=%08x\n", crc);
        rcp_bytes_t w = rcp_e2e_wrap_framed(0x0102030405060708ULL, true, 0x80, false, 0, f[2].data, f[2].len);
        size_t real_len = f[2].len - (f[2].data[2] >> 6);
        w.data[real_len] = (uint8_t)(crc >> 24); w.data[real_len+1] = (uint8_t)(crc >> 16);
        w.data[real_len+2] = (uint8_t)(crc >> 8); w.data[real_len+3] = (uint8_t)crc;
        hex("v3 frag2 wire", w.data, w.len);
    }
    /* compute_crc known answers */
    {
        const uint8_t acf[4] = {1,2,3,4};
        printf("kat compute_crc=%08x\n", rcp_e2e_compute_crc(0x05, 0x81, true, 0x0102030405060708ULL, 0x11223344u, acf, 4));
        printf("kat compute_crc_empty=%08x\n", rcp_e2e_compute_crc(0x82, 0x80, false, 0, 0, NULL, 0));
    }
    return 0;
}
