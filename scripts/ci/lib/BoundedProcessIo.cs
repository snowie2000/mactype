using System;
using System.Collections.Generic;
using System.IO;
using System.Text;
using System.Threading;
using System.Threading.Tasks;

namespace MacType.ControlCenter.Ci
{
    internal static class BoundedProcessIo
    {
        public static void WaitForCompletion(
            WindowsProcessJob job,
            Task exitTask,
            Task inputTask,
            Task<string> standardOutputTask,
            Task<string> standardErrorTask,
            Task timeoutTask)
        {
            bool exitDone = false;
            bool inputDone = false;
            bool standardOutputDone = false;
            bool standardErrorDone = false;
            while (true)
            {
                if (timeoutTask.IsCompleted)
                {
                    throw new OverallDeadlineExceededException();
                }
                if (!exitDone && exitTask.IsCompleted)
                {
                    exitTask.GetAwaiter().GetResult();
                    exitDone = true;
                }
                if (!inputDone && inputTask.IsCompleted)
                {
                    inputTask.GetAwaiter().GetResult();
                    inputDone = true;
                }
                if (!standardOutputDone && standardOutputTask.IsCompleted)
                {
                    standardOutputTask.GetAwaiter().GetResult();
                    standardOutputDone = true;
                }
                if (!standardErrorDone && standardErrorTask.IsCompleted)
                {
                    standardErrorTask.GetAwaiter().GetResult();
                    standardErrorDone = true;
                }

                if (exitDone && inputDone && standardOutputDone &&
                    standardErrorDone && job.ActiveProcessCount == 0)
                {
                    return;
                }

                var pending = new List<Task> { timeoutTask };
                if (!exitDone)
                {
                    pending.Add(exitTask);
                }
                if (!inputDone)
                {
                    pending.Add(inputTask);
                }
                if (!standardOutputDone)
                {
                    pending.Add(standardOutputTask);
                }
                if (!standardErrorDone)
                {
                    pending.Add(standardErrorTask);
                }

                if (pending.Count == 1)
                {
                    using (var pollCancellation = new CancellationTokenSource())
                    {
                        Task poll = Task.Delay(10, pollCancellation.Token);
                        Task.WhenAny(timeoutTask, poll).GetAwaiter().GetResult();
                        CancelAndDisposeDelay(pollCancellation, poll);
                    }
                }
                else
                {
                    Task.WhenAny(pending).GetAwaiter().GetResult();
                }
            }
        }

        public static async Task WriteInputAsync(Stream stream, byte[] input)
        {
            try
            {
                if (input != null && input.Length != 0)
                {
                    await stream.WriteAsync(input.AsMemory()).ConfigureAwait(false);
                    await stream.FlushAsync().ConfigureAwait(false);
                }
            }
            finally
            {
                stream.Dispose();
            }
        }

        public static async Task<string> ReadBoundedAsync(
            Stream stream,
            string streamName,
            int maximumBytes)
        {
            using (var captured = new MemoryStream())
            {
                var buffer = new byte[4096];
                while (true)
                {
                    int remaining = maximumBytes + 1 - checked((int)captured.Length);
                    int read = await stream.ReadAsync(
                        buffer.AsMemory(0, Math.Min(buffer.Length, remaining)))
                        .ConfigureAwait(false);
                    if (read == 0)
                    {
                        return Encoding.UTF8.GetString(captured.ToArray());
                    }
                    captured.Write(buffer, 0, read);
                    if (captured.Length > maximumBytes)
                    {
                        throw new OutputLimitExceededException(streamName, maximumBytes);
                    }
                }
            }
        }

        public static bool SettleTasks(
            IEnumerable<Task> tasks,
            int timeoutMilliseconds)
        {
            var pending = new List<Task>();
            foreach (Task task in tasks)
            {
                if (task != null && !task.IsCompleted)
                {
                    pending.Add(task);
                }
            }
            if (pending.Count != 0)
            {
                Task all = Task.WhenAll(pending);
                using (var cancellation = new CancellationTokenSource())
                {
                    Task timeout = Task.Delay(timeoutMilliseconds, cancellation.Token);
                    Task completed = Task.WhenAny(all, timeout).GetAwaiter().GetResult();
                    CancelAndDisposeDelay(cancellation, timeout);
                    if (completed != all)
                    {
                        return false;
                    }
                }
                DisposeCompleted(all);
            }
            foreach (Task task in tasks)
            {
                ObserveCompleted(task);
            }
            return true;
        }

        public static void DisposeCompleted(Task task)
        {
            if (task != null && task.IsCompleted)
            {
                task.Dispose();
            }
        }

        private static void CancelAndDisposeDelay(
            CancellationTokenSource cancellation,
            Task delay)
        {
            cancellation.Cancel();
            if (!SpinWait.SpinUntil(() => delay.IsCompleted, 100))
            {
                throw new InvalidOperationException(
                    "Setup process deadline task did not settle after cancellation.");
            }
            ObserveCompleted(delay);
            delay.Dispose();
        }

        private static void ObserveCompleted(Task task)
        {
            if (task == null || !task.IsCompleted)
            {
                return;
            }
            try
            {
                task.GetAwaiter().GetResult();
            }
            catch (OperationCanceledException)
            {
            }
            catch
            {
            }
        }
    }
}
