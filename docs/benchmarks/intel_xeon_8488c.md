# ☁️ Intel Xeon Platinum 8488C (AWS c7i.large)

*   **Processor:** Intel(R) Xeon(R) Platinum 8488C
*   **Environment:** AWS `c7i.large` (2 vCPU, 4GB RAM)
*   **Instruction Sets:** AVX512, AVX2

## 📊 Results

```text
Benchmarking Base58_Performances/Encode/Turbo/16
  time:   [33.747 ns 33.757 ns 33.768 ns]
  thrpt:  [451.88 MiB/s 452.02 MiB/s 452.15 MiB/s]

Benchmarking Base58_Performances/Encode/bs58/16
  time:   [238.96 ns 238.99 ns 239.04 ns]
  thrpt:  [63.835 MiB/s 63.847 MiB/s 63.856 MiB/s]

Benchmarking Base58_Performances/Encode/base58/16
  time:   [273.49 ns 275.33 ns 277.89 ns]
  thrpt:  [54.909 MiB/s 55.421 MiB/s 55.792 MiB/s]

Benchmarking Base58_Performances/Decode/Turbo/16
  time:   [24.253 ns 24.258 ns 24.263 ns]
  thrpt:  [864.73 MiB/s 864.91 MiB/s 865.06 MiB/s]

Benchmarking Base58_Performances/Decode/bs58/16
  time:   [96.174 ns 96.209 ns 96.245 ns]
  thrpt:  [217.99 MiB/s 218.08 MiB/s 218.16 MiB/s]

Benchmarking Base58_Performances/Decode/base58/16
  time:   [466.14 ns 466.43 ns 466.72 ns]
  thrpt:  [44.954 MiB/s 44.982 MiB/s 45.010 MiB/s]

Benchmarking Base58_Performances/Encode/Turbo/24
  time:   [50.835 ns 50.838 ns 50.842 ns]
  thrpt:  [450.19 MiB/s 450.22 MiB/s 450.24 MiB/s]

Benchmarking Base58_Performances/Encode/bs58/24
  time:   [475.04 ns 475.08 ns 475.13 ns]
  thrpt:  [48.173 MiB/s 48.177 MiB/s 48.181 MiB/s]

Benchmarking Base58_Performances/Encode/base58/24
  time:   [518.67 ns 518.90 ns 519.16 ns]
  thrpt:  [44.087 MiB/s 44.109 MiB/s 44.129 MiB/s]

Benchmarking Base58_Performances/Decode/Turbo/24
  time:   [31.761 ns 31.764 ns 31.768 ns]
  thrpt:  [990.65 MiB/s 990.78 MiB/s 990.89 MiB/s]

Benchmarking Base58_Performances/Decode/bs58/24
  time:   [181.98 ns 183.69 ns 187.51 ns]
  thrpt:  [167.84 MiB/s 171.33 MiB/s 172.94 MiB/s]

Benchmarking Base58_Performances/Decode/base58/24
  time:   [668.09 ns 670.06 ns 673.66 ns]
  thrpt:  [46.717 MiB/s 46.968 MiB/s 47.106 MiB/s]

Benchmarking Base58_Performances/Encode/Turbo/25
  time:   [36.397 ns 36.407 ns 36.418 ns]
  thrpt:  [654.68 MiB/s 654.86 MiB/s 655.06 MiB/s]

Benchmarking Base58_Performances/Encode/bs58/25
  time:   [531.73 ns 531.79 ns 531.85 ns]
  thrpt:  [44.828 MiB/s 44.834 MiB/s 44.838 MiB/s]

Benchmarking Base58_Performances/Encode/base58/25
  time:   [565.93 ns 566.34 ns 566.73 ns]
  thrpt:  [42.069 MiB/s 42.098 MiB/s 42.129 MiB/s]

Benchmarking Base58_Performances/Decode/Turbo/25
  time:   [35.939 ns 35.943 ns 35.946 ns]
  thrpt:  [928.57 MiB/s 928.66 MiB/s 928.76 MiB/s]

Benchmarking Base58_Performances/Decode/bs58/25
  time:   [200.17 ns 200.25 ns 200.39 ns]
  thrpt:  [166.57 MiB/s 166.69 MiB/s 166.76 MiB/s]

Benchmarking Base58_Performances/Decode/base58/25
  time:   [709.59 ns 710.18 ns 710.76 ns]
  thrpt:  [46.962 MiB/s 47.000 MiB/s 47.039 MiB/s]

Benchmarking Base58_Performances/Encode/Turbo/32
  time:   [43.258 ns 43.265 ns 43.272 ns]
  thrpt:  [705.26 MiB/s 705.36 MiB/s 705.47 MiB/s]

Benchmarking Base58_Performances/Encode/bs58/32
  time:   [885.08 ns 885.40 ns 886.04 ns]
  thrpt:  [34.443 MiB/s 34.468 MiB/s 34.480 MiB/s]

Benchmarking Base58_Performances/Encode/base58/32
  time:   [849.49 ns 849.79 ns 850.13 ns]
  thrpt:  [35.898 MiB/s 35.912 MiB/s 35.924 MiB/s]

Benchmarking Base58_Performances/Encode/five8/32
  time:   [64.722 ns 64.732 ns 64.745 ns]
  thrpt:  [471.35 MiB/s 471.44 MiB/s 471.52 MiB/s]

Benchmarking Base58_Performances/Decode/Turbo/32
  time:   [38.546 ns 38.564 ns 38.599 ns]
  thrpt:  [1.0616 GiB/s 1.0626 GiB/s 1.0631 GiB/s]

Benchmarking Base58_Performances/Decode/bs58/32
  time:   [313.15 ns 313.20 ns 313.25 ns]
  thrpt:  [133.96 MiB/s 133.98 MiB/s 134.00 MiB/s]

Benchmarking Base58_Performances/Decode/base58/32
  time:   [870.54 ns 871.22 ns 871.87 ns]
  thrpt:  [48.128 MiB/s 48.164 MiB/s 48.202 MiB/s]

Benchmarking Base58_Performances/Decode/five8/32
  time:   [52.215 ns 52.222 ns 52.229 ns]
  thrpt:  [803.42 MiB/s 803.53 MiB/s 803.63 MiB/s]

Benchmarking Base58_Performances/Encode/Turbo/48
  time:   [98.972 ns 98.999 ns 99.043 ns]
  thrpt:  [462.19 MiB/s 462.39 MiB/s 462.52 MiB/s]

Benchmarking Base58_Performances/Encode/bs58/48
  time:   [2.0962 µs 2.0963 µs 2.0964 µs]
  thrpt:  [21.835 MiB/s 21.836 MiB/s 21.837 MiB/s]

Benchmarking Base58_Performances/Encode/base58/48
  time:   [1.8856 µs 1.8866 µs 1.8877 µs]
  thrpt:  [24.250 MiB/s 24.264 MiB/s 24.277 MiB/s]

Benchmarking Base58_Performances/Decode/Turbo/48
  time:   [55.210 ns 55.214 ns 55.219 ns]
  thrpt:  [1.1132 GiB/s 1.1133 GiB/s 1.1133 GiB/s]

Benchmarking Base58_Performances/Decode/bs58/48
  time:   [692.04 ns 692.12 ns 692.21 ns]
  thrpt:  [90.930 MiB/s 90.942 MiB/s 90.952 MiB/s]

Benchmarking Base58_Performances/Decode/base58/48
  time:   [1.2832 µs 1.2839 µs 1.2845 µs]
  thrpt:  [49.001 MiB/s 49.026 MiB/s 49.053 MiB/s]

Benchmarking Base58_Performances/Encode/Turbo/64
  time:   [110.91 ns 110.93 ns 110.94 ns]
  thrpt:  [550.14 MiB/s 550.23 MiB/s 550.32 MiB/s]

Benchmarking Base58_Performances/Encode/bs58/64
  time:   [3.9737 µs 3.9738 µs 3.9740 µs]
  thrpt:  [15.359 MiB/s 15.359 MiB/s 15.360 MiB/s]

Benchmarking Base58_Performances/Encode/base58/64
  time:   [3.2467 µs 3.2494 µs 3.2513 µs]
  thrpt:  [18.772 MiB/s 18.784 MiB/s 18.799 MiB/s]

Benchmarking Base58_Performances/Encode/five8/64
  time:   [200.81 ns 200.81 ns 200.82 ns]
  thrpt:  [303.93 MiB/s 303.94 MiB/s 303.95 MiB/s]

Benchmarking Base58_Performances/Decode/Turbo/64
  time:   [74.110 ns 74.117 ns 74.127 ns]
  thrpt:  [1.1056 GiB/s 1.1058 GiB/s 1.1059 GiB/s]

Benchmarking Base58_Performances/Decode/bs58/64
  time:   [1.2515 µs 1.2516 µs 1.2518 µs]
  thrpt:  [67.043 MiB/s 67.052 MiB/s 67.060 MiB/s]

Benchmarking Base58_Performances/Decode/base58/64
  time:   [1.6982 µs 1.6997 µs 1.7012 µs]
  thrpt:  [49.332 MiB/s 49.376 MiB/s 49.420 MiB/s]

Benchmarking Base58_Performances/Decode/five8/64
  time:   [197.01 ns 198.28 ns 199.21 ns]
  thrpt:  [421.29 MiB/s 423.26 MiB/s 425.99 MiB/s]

Benchmarking Base58_Performances/Encode/Turbo/69
  time:   [233.30 ns 233.38 ns 233.50 ns]
  thrpt:  [281.81 MiB/s 281.96 MiB/s 282.06 MiB/s]

Benchmarking Base58_Performances/Encode/bs58/69
  time:   [4.6874 µs 4.6882 µs 4.6888 µs]
  thrpt:  [14.034 MiB/s 14.036 MiB/s 14.038 MiB/s]

Benchmarking Base58_Performances/Encode/base58/69
  time:   [3.7663 µs 3.7733 µs 3.7807 µs]
  thrpt:  [17.405 MiB/s 17.439 MiB/s 17.471 MiB/s]

Benchmarking Base58_Performances/Decode/Turbo/69
  time:   [83.459 ns 83.465 ns 83.475 ns]
  thrpt:  [1.0599 GiB/s 1.0600 GiB/s 1.0601 GiB/s]

Benchmarking Base58_Performances/Decode/bs58/69
  time:   [1.4592 µs 1.4594 µs 1.4596 µs]
  thrpt:  [62.070 MiB/s 62.079 MiB/s 62.088 MiB/s]

Benchmarking Base58_Performances/Decode/base58/69
  time:   [1.8303 µs 1.8317 µs 1.8329 µs]
  thrpt:  [49.428 MiB/s 49.462 MiB/s 49.500 MiB/s]

Benchmarking Base58_Performances/Encode/Turbo/128
  time:   [550.01 ns 550.13 ns 550.27 ns]
  thrpt:  [221.84 MiB/s 221.89 MiB/s 221.94 MiB/s]

Benchmarking Base58_Performances/Encode/bs58/128
  time:   [17.394 µs 17.400 µs 17.407 µs]
  thrpt:  [7.0126 MiB/s 7.0154 MiB/s 7.0178 MiB/s]

Benchmarking Base58_Performances/Encode/base58/128
  time:   [13.894 µs 13.896 µs 13.898 µs]
  thrpt:  [8.7830 MiB/s 8.7843 MiB/s 8.7856 MiB/s]

Benchmarking Base58_Performances/Decode/Turbo/128
  time:   [175.26 ns 175.27 ns 175.29 ns]
  thrpt:  [952.09 MiB/s 952.19 MiB/s 952.27 MiB/s]

Benchmarking Base58_Performances/Decode/bs58/128
  time:   [5.0900 µs 5.0907 µs 5.0913 µs]
  thrpt:  [32.780 MiB/s 32.784 MiB/s 32.788 MiB/s]

Benchmarking Base58_Performances/Decode/base58/128
  time:   [3.3295 µs 3.3319 µs 3.3344 µs]
  thrpt:  [50.052 MiB/s 50.089 MiB/s 50.126 MiB/s]

Model name: Intel(R) Xeon(R) Platinum 8488C
```
