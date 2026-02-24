## Debugging

Debugging process:

1. replicate the bug by creating an integration test in tests/.
    1. The test must closely resemble the end user behavior.
    2. Only stub external API requests.
2. execute the test and verify that it replicates the bug.
3. Then fix the bug
4. Explain the problem and fix
    a. Problem: explanation of the relevant part of the code/system behavior that was responsible for the bug.
    b. Fix: your solution
    c. Ensure the summary is concise, self-contained, and scannable (to the reader)

