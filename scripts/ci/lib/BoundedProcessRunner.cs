using System;
using System.ComponentModel;
using System.Diagnostics;
using System.IO;
using System.Text;
using System.Threading;
using System.Threading.Tasks;

namespace MacType.ControlCenter.Ci
{
    public sealed class BoundedProcessResult
    {
        public BoundedProcessResult(int exitCode, string standardOutput, string standardError)
        {
            ExitCode = exitCode;
            StandardOutput = standardOutput;
            StandardError = standardError;
        }

        public int ExitCode { get; }
        public string StandardOutput { get; }
        public string StandardError { get; }
    }

    internal sealed class OutputLimitExceededException : IOException
    {
        public OutputLimitExceededException(string streamName, int maximumBytes)
            : base($"Setup process {streamName} exceeded {maximumBytes} bytes.")
        {
        }
    }

    internal sealed class OverallDeadlineExceededException : TimeoutException
    {
    }

    public static class BoundedProcessRunner
    {
        private const string JobAssignmentReceiptEnvironment =
            "MACTYPE_CI_PROCESS_GROUP_READY_FILE";
        private const int MaximumOverallTimeoutMilliseconds = 10 * 60 * 1000;
        private const int MaximumOutputCaptureBytes = 1024 * 1024;
        private const int MaximumTerminationTimeoutMilliseconds = 60 * 1000;
        private const int MaximumInputBytes = 4 * 1024 * 1024;

        public static BoundedProcessResult Run(
            string executable,
            string argument,
            byte[] input,
            int timeoutMilliseconds,
            int maximumOutputBytes,
            int terminationTimeoutMilliseconds)
        {
            return RunArguments(
                executable,
                new[] { argument },
                input,
                timeoutMilliseconds,
                maximumOutputBytes,
                terminationTimeoutMilliseconds);
        }

        public static BoundedProcessResult RunArguments(
            string executable,
            string[] arguments,
            byte[] input,
            int timeoutMilliseconds,
            int maximumOutputBytes,
            int terminationTimeoutMilliseconds)
        {
            ValidateArguments(
                executable,
                arguments,
                input,
                timeoutMilliseconds,
                maximumOutputBytes,
                terminationTimeoutMilliseconds);
            string assignmentReceiptPath = Path.Combine(
                Path.GetTempPath(),
                $"mactype-ci-process-group-{Guid.NewGuid():N}.ready");
            Process process = null;
            WindowsProcessJob job = null;
            CancellationTokenSource timeoutCancellation = null;
            CancellationTokenSource exitCancellation = null;
            Task<string> standardOutputTask = null;
            Task<string> standardErrorTask = null;
            Task inputTask = null;
            Task exitTask = null;
            Stream standardInputStream = null;
            Stream standardOutputStream = null;
            Stream standardErrorStream = null;
            Task timeoutTask = null;
            bool started = false;
            try
            {
                var startInfo = new ProcessStartInfo
                {
                    FileName = executable,
                    UseShellExecute = false,
                    CreateNoWindow = true,
                    RedirectStandardOutput = true,
                    RedirectStandardError = true,
                    RedirectStandardInput = true,
                };
                foreach (string argument in arguments)
                {
                    startInfo.ArgumentList.Add(argument);
                }
                startInfo.Environment[JobAssignmentReceiptEnvironment] =
                    assignmentReceiptPath;

                process = new Process { StartInfo = startInfo };
                job = new WindowsProcessJob();
                timeoutCancellation = new CancellationTokenSource();
                exitCancellation = new CancellationTokenSource();
                timeoutTask = Task.Delay(
                    timeoutMilliseconds,
                    timeoutCancellation.Token);
                if (!process.Start())
                {
                    throw new InvalidOperationException("Could not start setup process.");
                }
                started = true;
                try
                {
                    job.Assign(process);
                }
                catch (Exception error)
                {
                    throw new InvalidOperationException(
                        "Setup process job assignment failed; execution was refused fail-closed.",
                        error);
                }
                File.WriteAllText(
                    assignmentReceiptPath,
                    "assigned",
                    new UTF8Encoding(encoderShouldEmitUTF8Identifier: false));

                standardInputStream = process.StandardInput.BaseStream;
                standardOutputStream = process.StandardOutput.BaseStream;
                standardErrorStream = process.StandardError.BaseStream;
                standardOutputTask = BoundedProcessIo.ReadBoundedAsync(
                    standardOutputStream,
                    "stdout",
                    maximumOutputBytes);
                standardErrorTask = BoundedProcessIo.ReadBoundedAsync(
                    standardErrorStream,
                    "stderr",
                    maximumOutputBytes);
                inputTask = BoundedProcessIo.WriteInputAsync(standardInputStream, input);
                exitTask = process.WaitForExitAsync(exitCancellation.Token);

                BoundedProcessIo.WaitForCompletion(
                    job,
                    exitTask,
                    inputTask,
                    standardOutputTask,
                    standardErrorTask,
                    timeoutTask);

                return new BoundedProcessResult(
                    process.ExitCode,
                    standardOutputTask.GetAwaiter().GetResult(),
                    standardErrorTask.GetAwaiter().GetResult());
            }
            catch (OutputLimitExceededException error)
            {
                TerminateAndConfirm(
                    process,
                    job,
                    started,
                    terminationTimeoutMilliseconds,
                    error.Message);
                throw new InvalidOperationException(
                    $"{error.Message} The process tree was terminated.",
                    error);
            }
            catch (OverallDeadlineExceededException error)
            {
                TerminateAndConfirm(
                    process,
                    job,
                    started,
                    terminationTimeoutMilliseconds,
                    "Setup process timed out.");
                throw new TimeoutException(
                    "Setup process timed out; the process tree was terminated.",
                    error);
            }
            catch
            {
                if (started)
                {
                    TerminateAndConfirm(
                        process,
                        job,
                        started,
                        terminationTimeoutMilliseconds,
                        "Setup process failed before a terminal result.");
                }
                throw;
            }
            finally
            {
                timeoutCancellation?.Cancel();
                exitCancellation?.Cancel();
                job?.Dispose();
                if (started)
                {
                    DisposeIgnoringClosed(standardInputStream);
                    DisposeIgnoringClosed(standardOutputStream);
                    DisposeIgnoringClosed(standardErrorStream);
                }
                process?.Dispose();

                Task[] tasks =
                {
                    standardOutputTask,
                    standardErrorTask,
                    inputTask,
                    exitTask,
                    timeoutTask,
                };
                bool settled = false;
                try
                {
                    settled = BoundedProcessIo.SettleTasks(
                        tasks,
                        terminationTimeoutMilliseconds);
                }
                finally
                {
                    foreach (Task task in tasks)
                    {
                        BoundedProcessIo.DisposeCompleted(task);
                    }
                    timeoutCancellation?.Dispose();
                    exitCancellation?.Dispose();
                    TryDelete(assignmentReceiptPath);
                }
                if (!settled)
                {
                    throw new InvalidOperationException(
                        "Setup process asynchronous cleanup did not settle within the fixed deadline.");
                }
            }
        }

        private static void ValidateArguments(
            string executable,
            string[] arguments,
            byte[] input,
            int timeoutMilliseconds,
            int maximumOutputBytes,
            int terminationTimeoutMilliseconds)
        {
            if (string.IsNullOrWhiteSpace(executable))
            {
                throw new ArgumentException(
                    "Setup executable must not be empty.",
                    nameof(executable));
            }
            if (arguments == null)
            {
                throw new ArgumentNullException(nameof(arguments));
            }
            for (int index = 0; index < arguments.Length; index++)
            {
                if (arguments[index] == null)
                {
                    throw new ArgumentException(
                        $"Setup argument at index {index} must not be null.",
                        nameof(arguments));
                }
            }
            ValidatePositiveBounded(
                timeoutMilliseconds,
                MaximumOverallTimeoutMilliseconds,
                nameof(timeoutMilliseconds));
            ValidatePositiveBounded(
                maximumOutputBytes,
                MaximumOutputCaptureBytes,
                nameof(maximumOutputBytes));
            ValidatePositiveBounded(
                terminationTimeoutMilliseconds,
                MaximumTerminationTimeoutMilliseconds,
                nameof(terminationTimeoutMilliseconds));
            if (input != null && input.Length > MaximumInputBytes)
            {
                throw new ArgumentOutOfRangeException(
                    nameof(input),
                    $"Setup input must not exceed {MaximumInputBytes} bytes.");
            }
        }

        private static void ValidatePositiveBounded(
            int value,
            int maximum,
            string parameterName)
        {
            if (value < 1 || value > maximum)
            {
                throw new ArgumentOutOfRangeException(
                    parameterName,
                    $"Value must be between 1 and {maximum}.");
            }
        }

        private static void TerminateAndConfirm(
            Process process,
            WindowsProcessJob job,
            bool started,
            int terminationTimeoutMilliseconds,
            string reason)
        {
            var deadline = Stopwatch.StartNew();
            if (started && !process.HasExited)
            {
                try
                {
                    process.Kill(entireProcessTree: true);
                }
                catch (InvalidOperationException)
                {
                }
                catch (Win32Exception)
                {
                }
            }
            job.Terminate();

            if (started && !process.HasExited)
            {
                int remaining = RemainingMilliseconds(
                    deadline,
                    terminationTimeoutMilliseconds);
                if (remaining <= 0 || !process.WaitForExit(remaining))
                {
                    throw new InvalidOperationException(
                        $"{reason} Parent-process termination could not be confirmed within " +
                        $"{terminationTimeoutMilliseconds} ms.");
                }
            }

            int jobRemaining = RemainingMilliseconds(
                deadline,
                terminationTimeoutMilliseconds);
            if (jobRemaining <= 0 || !job.WaitForEmpty(jobRemaining))
            {
                throw new InvalidOperationException(
                    $"{reason} Process-tree termination could not be confirmed within " +
                    $"{terminationTimeoutMilliseconds} ms.");
            }
        }

        private static int RemainingMilliseconds(Stopwatch elapsed, int limit)
        {
            long remaining = limit - elapsed.ElapsedMilliseconds;
            return remaining <= 0 ? 0 : checked((int)Math.Min(remaining, int.MaxValue));
        }

        private static void TryDelete(string path)
        {
            try
            {
                File.Delete(path);
            }
            catch (IOException)
            {
            }
            catch (UnauthorizedAccessException)
            {
            }
        }

        private static void DisposeIgnoringClosed(IDisposable resource)
        {
            if (resource == null)
            {
                return;
            }
            try
            {
                resource.Dispose();
            }
            catch (ObjectDisposedException)
            {
            }
            catch (IOException)
            {
            }
        }
    }

}
