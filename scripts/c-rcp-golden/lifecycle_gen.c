/* SPDX-License-Identifier: MPL-2.0
 *
 * Golden-vector generator for src/lifecycle.rs / src/regmap.rs parity tests
 * (testdata/c_rcp_lifecycle_golden.txt). Runs c-RCP's real lifecycle.c and
 * regmap.c over an exhaustive input grid and prints one result string per
 * function. Regenerate with:
 *
 *   cmake -S <c-RCP> -B /tmp/crcp-build && cmake --build /tmp/crcp-build
 *   cc -std=c99 -I<c-RCP>/include scripts/c-rcp-golden/lifecycle_gen.c \
 *      /tmp/crcp-build/librcp.a -o /tmp/lifecycle_gen
 *   /tmp/lifecycle_gen > testdata/c_rcp_lifecycle_golden.txt
 *
 * Index order of every grid must match the iteration order in the rust
 * tests (lifecycle.rs `c_rcp_golden_*`).
 */
#include "rcp/lifecycle.h"
#include "rcp/regmap.h"
#include <stdio.h>
#include <string.h>

typedef rcp_lifecycle_endpoint_plausibility_t EP;
typedef rcp_lifecycle_request_stream_plausibility_t RS;
#define NSNAP 11
static EP eps[NSNAP][2];
static RS rss[NSNAP][2];
static rcp_lifecycle_plausibility_snapshot_t snaps[NSNAP];
static void mk(void)
{
    EP good  = {true, true, true, true, 0};
    RS rgood = {true, true, 0};
    for (int i = 1; i < NSNAP; i++) {
        eps[i][0]                     = good;
        rss[i][0]                     = rgood;
        snaps[i].endpoints            = eps[i];
        snaps[i].endpoint_count       = 1;
        snaps[i].request_streams      = rss[i];
        snaps[i].request_stream_count = 1;
        snaps[i].response_stream_count = 1;
    }
    eps[2][0].hw_pin_mapped      = false;
    eps[3][0].has_request_stream = false;
    eps[4][0].has_stream_assoc   = false;
    rss[5][0].has_response_stream = false;
    snaps[6].response_stream_count = 0;
    rss[7][1]                     = rgood;
    snaps[7].request_stream_count = 2;
    snaps[8].endpoint_count       = 0;
    snaps[8].request_stream_count = 0;
    memset(&eps[9][0], 0, sizeof(EP));
    snaps[9].request_stream_count = 0;
    eps[10][0].ep_used       = false;
    eps[10][0].hw_pin_mapped = false;
}
static rcp_lifecycle_writer_ctx_t w(int b)
{
    rcp_lifecycle_writer_ctx_t x = {0};
    x.via_root_client_ep0          = b & 1;
    x.via_owning_stream            = (b >> 1) & 1;
    x.via_non_unicast_frame        = (b >> 2) & 1;
    x.via_discovery_stream         = (b >> 3) & 1;
    x.via_valid_stream_association = (b >> 4) & 1;
    return x;
}
static const rcp_lifecycle_state_t ST[3] = {RCP_LIFECYCLE_HW_UNCONFIGURED, RCP_LIFECYCLE_HW_CONFIGURED,
                                            RCP_LIFECYCLE_RCP_CONFIGURED};
static char werr(rcp_wire_error_t e)
{
    return e == RCP_ERROR_NONE                ? '0'
           : e == RCP_ERROR_LOCKED_MEM_ACCESS ? 'L'
           : e == RCP_ERROR_UNAUTHORIZED_ACCESS ? 'U'
                                                : '?';
}
int main(void)
{
    mk();
    printf("HW_CFG ");
    for (int s = 0; s < NSNAP; s++) printf("%d", rcp_lifecycle_check_hw_cfg(s ? &snaps[s] : NULL));
    printf("\nRCP_CFG ");
    for (int s = 0; s < NSNAP; s++) printf("%d", rcp_lifecycle_check_rcp_cfg(s ? &snaps[s] : NULL));
    printf("\nTRANSITION ");
    for (int f = 0; f < 3; f++)
        for (int t = 0; t < 3; t++)
            for (int s = 0; s < NSNAP; s++)
                for (int b = 0; b < 32; b++)
                    for (int i = 0; i < 2; i++) {
                        rcp_lifecycle_state_t st = ST[f];
                        rcp_lifecycle_errc_t  rc =
                            rcp_lifecycle_transition(&st, ST[t], s ? &snaps[s] : NULL, w(b), i);
                        printf("%d", rc);
                        if ((rc == 0) != (st == ST[t]) && f != t) {
                            fprintf(stderr, "state mismatch\n");
                            return 1;
                        }
                        if (rc != 0 && st != ST[f]) {
                            fprintf(stderr, "mutated on failure\n");
                            return 1;
                        }
                    }
    static const uint8_t  SUB[3] = {0x05, 0x82, 0x00};
    static const uint8_t  MT[3]  = {0x0E, 0x0D, 0x00};
    static const uint16_t BB[3]  = {0, 1, 0x7FF};
    printf("\nACCEPT ");
    for (int s = 0; s < 3; s++)
        for (int ts = 0; ts < 2; ts++)
            for (int a = 0; a < 3; a++)
                for (int m = 0; m < 3; m++)
                    for (int bb = 0; bb < 3; bb++)
                        for (int p = 0; p < 2; p++)
                            printf("%d", rcp_lifecycle_should_accept(
                                             ST[s], ts, SUB[a], MT[m], BB[bb],
                                             p ? RCP_AVTP_TSCF_FALLBACK_IGNORE : RCP_AVTP_TSCF_FALLBACK_DROP));
    printf("\nFIELD ");
    for (int s = 0; s < 3; s++)
        for (int k = 0; k < 4; k++)
            for (int b = 0; b < 32; b++)
                printf("%c", werr(rcp_lifecycle_field_write_error(ST[s], (rcp_lifecycle_field_kind_t)k, w(b))));
    printf("\nWPLUS ");
    for (int s = 0; s < 3; s++)
        for (int b = 0; b < 32; b++)
            for (int l = 0; l < 2; l++) printf("%c", werr(rcp_lifecycle_field_write_error_w_plus(ST[s], w(b), l)));
    printf("\n");
    for (int i = 0; i <= 6; i++) printf("STRERR%d %s\n", i, rcp_lifecycle_strerror((rcp_lifecycle_errc_t)i));

    /* General-register write request (REQ-RMAP-025). */
    {
        rcp_wire_error_t err = RCP_ERROR_NONE;
        uint8_t          txn = 0;
        uint8_t          payload[4] = {1, 2, 3, 4};
        rcp_acf_byte_message_info_t hdr = {0};
        rcp_bytes_t f;
        hdr.byte_bus_id = 0;
        hdr.op = RCP_ACF_OP_WRITE;
        hdr.transaction_num = 0x5A;
        hdr.read_size_or_segment_num = 0;
        f = rcp_acf_encode_abb(&hdr, payload, 4);
        printf("GENWR_FRAME ");
        for (size_t i = 0; i < f.len; i++) printf("%02X", f.data[i]);
        printf("\nGENWR %d %d 0x%02X\n", rcp_regmap_general_decode_write_request(f.data, f.len, &err, &txn), err, txn);
        printf("GENWR_SHORT %d\n", rcp_regmap_general_decode_write_request(f.data, 4, &err, &txn));
        /* Malformed acf_msg_length / pad on an otherwise-golden frame
         * (rcp_acf_decode_abb returns SHORT_FRAME for all of these). */
        {
            uint8_t m[12];
            memcpy(m, f.data, sizeof m);
            m[1] = 0x00; /* acf_msg_length 0: total 0 < header */
            printf("GENWR_LEN0 %d\n", rcp_regmap_general_decode_write_request(m, sizeof m, &err, &txn));
            m[1] = 0x01; /* acf_msg_length 1: total 4 < header */
            printf("GENWR_LEN1 %d\n", rcp_regmap_general_decode_write_request(m, sizeof m, &err, &txn));
            m[1] = 0x02;               /* header only, body_len 0 */
            m[2] = (uint8_t)(m[2] | 0x40u); /* pad 1 > body_len 0 */
            printf("GENWR_PAD %d\n", rcp_regmap_general_decode_write_request(m, sizeof m, &err, &txn));
            memcpy(m, f.data, sizeof m);
            m[0] = (uint8_t)((RCP_ACF_MSG_TYPE_GBB << 1) | (m[0] & 1u));
            printf("GENWR_GBB %d\n", rcp_regmap_general_decode_write_request(m, sizeof m, &err, &txn));
        }
        rcp_bytes_free(&f);
        hdr.op = RCP_ACF_OP_READ;
        f = rcp_acf_encode_abb(&hdr, payload, 4);
        printf("GENWR_READ %d\n", rcp_regmap_general_decode_write_request(f.data, f.len, &err, &txn));
        rcp_bytes_free(&f);
        hdr.op = RCP_ACF_OP_WRITE;
        hdr.byte_bus_id = 1;
        f = rcp_acf_encode_abb(&hdr, payload, 4);
        printf("GENWR_BUS %d\n", rcp_regmap_general_decode_write_request(f.data, f.len, &err, &txn));
        rcp_bytes_free(&f);
        printf("LOCKED=%d UNAUTH=%d REJ=%d\n", RCP_ERROR_LOCKED_MEM_ACCESS, RCP_ERROR_UNAUTHORIZED_ACCESS,
               RCP_ERROR_REQUEST_REJECTED);
    }
    return 0;
}
