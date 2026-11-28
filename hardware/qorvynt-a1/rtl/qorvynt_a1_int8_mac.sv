// QORVYNT A1 — parameterized signed INT8 dot-product accumulator.
// This is a synthesizable compute primitive, not a complete accelerator.
module qorvynt_a1_int8_mac #(
    parameter integer LANES = 16
) (
    input  logic                         clk,
    input  logic                         rst,
    input  logic                         clear_acc,
    input  logic                         valid,
    input  logic [LANES*8-1:0]           a_packed,
    input  logic [LANES*8-1:0]           b_packed,
    output logic signed [31:0]           accumulator,
    output logic                         out_valid
);
    logic signed [31:0] dot_product;
    integer lane;

    always_comb begin
        dot_product = 32'sd0;
        for (lane = 0; lane < LANES; lane = lane + 1) begin
            dot_product = dot_product
                + ($signed(a_packed[lane*8 +: 8])
                * $signed(b_packed[lane*8 +: 8]));
        end
    end

    always_ff @(posedge clk) begin
        if (rst) begin
            accumulator <= 32'sd0;
            out_valid   <= 1'b0;
        end else if (clear_acc) begin
            accumulator <= 32'sd0;
            out_valid   <= 1'b0;
        end else begin
            out_valid <= valid;
            if (valid)
                accumulator <= accumulator + dot_product;
        end
    end
endmodule
