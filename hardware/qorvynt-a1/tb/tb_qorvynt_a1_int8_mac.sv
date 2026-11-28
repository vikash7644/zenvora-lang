`timescale 1ns/1ps

module tb_qorvynt_a1_int8_mac;
    localparam integer LANES = 4;

    logic clk = 1'b0;
    logic rst = 1'b1;
    logic clear_acc = 1'b0;
    logic valid = 1'b0;
    logic [LANES*8-1:0] a_packed = '0;
    logic [LANES*8-1:0] b_packed = '0;
    logic signed [31:0] accumulator;
    logic out_valid;

    always #5 clk = ~clk;

    qorvynt_a1_int8_mac #(.LANES(LANES)) dut (
        .clk(clk),
        .rst(rst),
        .clear_acc(clear_acc),
        .valid(valid),
        .a_packed(a_packed),
        .b_packed(b_packed),
        .accumulator(accumulator),
        .out_valid(out_valid)
    );

    initial begin
        repeat (2) @(posedge clk);
        @(negedge clk);
        rst = 1'b0;

        // 1*2 + 2*3 + 3*4 + 4*5 = 40
        a_packed = {8'd4, 8'd3, 8'd2, 8'd1};
        b_packed = {8'd5, 8'd4, 8'd3, 8'd2};
        valid = 1'b1;
        @(posedge clk);
        #1;
        if (accumulator !== 32'sd40 || out_valid !== 1'b1)
            $fatal(1, "first dot product mismatch: acc=%0d valid=%b", accumulator, out_valid);

        // Add (-1*2) + (2*-3) + (0*7) + (1*1) = -7; total = 33.
        @(negedge clk);
        a_packed = {8'sd1, 8'sd0, 8'sd2, -8'sd1};
        b_packed = {8'sd1, 8'sd7, -8'sd3, 8'sd2};
        @(posedge clk);
        #1;
        if (accumulator !== 32'sd33 || out_valid !== 1'b1)
            $fatal(1, "signed dot product mismatch: acc=%0d valid=%b", accumulator, out_valid);

        @(negedge clk);
        valid = 1'b0;
        clear_acc = 1'b1;
        @(posedge clk);
        #1;
        if (accumulator !== 32'sd0 || out_valid !== 1'b0)
            $fatal(1, "clear mismatch: acc=%0d valid=%b", accumulator, out_valid);

        $display("PASS: QORVYNT A1 INT8 MAC directed test");
        $finish;
    end
endmodule
