using System;
using System.Diagnostics;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Reflection;
using System.Text;
using System.Windows;
using Microsoft.Win32;

namespace Rikai.Setup
{
    internal static class Program
    {
        [STAThread]
        public static int Main(string[] args)
        {
            bool unattended = args.Any(a => new[] {"/SILENT", "/VERYSILENT", "/?", "/HELP"}.Contains(a, StringComparer.OrdinalIgnoreCase));
            try
            {
                if (unattended)
                    using (var engine = new InstallEngine())
                        return engine.Run(args);
                var app = new Application();
                LoadStrings(app);
                var window = new SetupWindow(args);
                app.Run(window);
                return window.ExitCode;
            }
            catch (Exception error)
            {
                if (!unattended)
                    MessageBox.Show(error.GetBaseException().Message, "Rikai", MessageBoxButton.OK, MessageBoxImage.Error);
                return 1;
            }
        }

        internal static void LoadStrings(Application app)
        {
            string language = CultureInfo.CurrentUICulture.TwoLetterISOLanguageName;
            string[] locales = { "fr", "en", "es", "ru", "ja", "zh" };
            int column = Array.IndexOf(locales, language);
            if (column < 0) column = 1;
            using (var reader = new StreamReader(Assembly.GetExecutingAssembly().GetManifestResourceStream("Rikai.Strings")))
            {
                string row;
                while ((row = reader.ReadLine()) != null)
                {
                    var cells = row.Split('\t');
                    app.Resources[cells[0]] = cells[column + 1];
                }
            }
        }
    }

    internal sealed class InstallEngine : IDisposable
    {
        private readonly string folder = Path.Combine(Path.GetTempPath(), "RikaiSetup-" + Guid.NewGuid().ToString("N"));
        internal string ProgressFile { get { return Path.Combine(folder, "progress"); } }
        private volatile bool cancelled;

        internal InstallEngine() { Directory.CreateDirectory(folder); }

        internal static string DefaultDestination(string[] args)
        {
            var requested = args.LastOrDefault(a => a.StartsWith("/DIR=", StringComparison.OrdinalIgnoreCase));
            if (requested != null) return requested.Substring(5);
            const string key = @"HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Uninstall\{E2C87401-75B8-4209-8969-CCF5DE7B7855}_is1";
            var existing = Registry.GetValue(key, "InstallLocation", null) as string;
            return string.IsNullOrWhiteSpace(existing) ? Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "Programs", "Rikai") : existing;
        }

        // Windows splits arguments at whitespace and treats backslashes specially before a quote.
        internal static string Quote(string arg)
        {
            var result = new StringBuilder("\"");
            int slashes = 0;
            foreach (char c in arg)
            {
                if (c == '\\') { slashes++; continue; }
                if (c == '"') result.Append('\\', slashes + 1);
                result.Append('\\', slashes).Append(c);
                slashes = 0;
            }
            return result.Append('\\', slashes * 2).Append('"').ToString();
        }

        internal void Cancel()
        {
            cancelled = true;
            File.WriteAllText(Path.Combine(folder, "cancel"), "");
        }

        internal int Run(string[] arguments)
        {
            string path = Path.Combine(folder, "Rikai-Payload.exe");
            using (var resource = Assembly.GetExecutingAssembly().GetManifestResourceStream("Rikai.Payload"))
            using (var payload = new FileStream(path, FileMode.CreateNew, FileAccess.Write, FileShare.None))
                resource.CopyTo(payload);
            if (cancelled) return 2;
            var options = new ProcessStartInfo(path, string.Join(" ", arguments.Select(Quote)))
            {
                UseShellExecute = false,
                // Keep the caller's directory for relative /LOG and /LOADINF paths.
                WorkingDirectory = Environment.CurrentDirectory
            };
            using (var child = Process.Start(options))
            {
                child.WaitForExit();
                return child.ExitCode;
            }
        }

        public void Dispose()
        {
            try { Directory.Delete(folder, true); }
            catch (IOException) { }
            catch (UnauthorizedAccessException) { }
        }
    }
}
