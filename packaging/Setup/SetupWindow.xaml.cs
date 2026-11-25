using System;
using System.ComponentModel;
using System.Diagnostics;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Reflection;
using System.Threading.Tasks;
using System.Windows;
using System.Windows.Automation;
using System.Windows.Controls;
using System.Windows.Input;
using System.Windows.Interop;
using System.Windows.Markup;
using System.Windows.Media;
using System.Windows.Media.Animation;
using System.Windows.Shapes;
using System.Windows.Threading;
using Forms = System.Windows.Forms;
using FilePath = System.IO.Path;

namespace Rikai.Setup
{
    public partial class SetupWindow : Window
    {
        private enum Page { Welcome, Running, Stopping, Finished, Cancelled, Failed }
        private Page page;
        private readonly string[] arguments;
        private readonly DispatcherTimer poll = new DispatcherTimer { Interval = TimeSpan.FromMilliseconds(150) };
        private readonly string[] stages = { "Application", "Worker", "Shortcuts" };
        private InstallEngine engine;
        private int phase = -1, percent;
        private string destination, log;
        private bool closeAfterCancel;
        private CultureInfo culture;
        private static readonly string[] languageCodes = { "fr", "en", "es", "ru", "ja", "zh" };
        private static readonly string[] engineLanguages = { "french", "english", "spanish", "russian", "japanese", "english" };
        internal int ExitCode { get; private set; } = 2;

        public SetupWindow(string[] args)
        {
            InitializeComponent();
            arguments = args;
            string path = InstallEngine.DefaultDestination(args);
            string standard = FilePath.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "Programs", "Rikai");
            Destination.Text = string.Equals(path.TrimEnd('\\'), standard, StringComparison.OrdinalIgnoreCase)
                ? @"%LOCALAPPDATA%\Programs\Rikai" : path;

            int preferred = Array.IndexOf(languageCodes, CultureInfo.CurrentUICulture.TwoLetterISOLanguageName);
            string requested = args.LastOrDefault(arg => arg.StartsWith("/LANG=", StringComparison.OrdinalIgnoreCase));
            if (requested != null)
            {
                int mapped = Array.FindIndex(engineLanguages, name => string.Equals(name, requested.Substring(6), StringComparison.OrdinalIgnoreCase));
                if (mapped >= 0) preferred = mapped;
            }
            LocalePicker.SelectedIndex = preferred < 0 ? 1 : preferred;
            VersionLabel.Text = "Rikai " + Assembly.GetExecutingAssembly().GetName().Version.ToString(3) + " · x64";
            MaxWidth = SystemParameters.WorkArea.Width;
            Track.SizeChanged += (s, e) => DrawProgress(false);
            poll.Tick += (s, e) => ReadProgress();
            Closing += OnClosing;
        }

        private void SelectLanguage(object sender, SelectionChangedEventArgs e)
        {
            var item = ((ComboBox)sender).SelectedItem as ComboBoxItem;
            if (item == null) return;
            string code = (string)item.Tag;
            culture = CultureInfo.GetCultureInfo(code == "zh" ? "zh-CN" : code);
            Language = XmlLanguage.GetLanguage(culture.Name);
            Program.LoadStrings(Application.Current, code);
        }

        private void ClearPathError(object sender, TextChangedEventArgs e)
        {
            if (PathHint != null) PathHint.Visibility = Visibility.Collapsed;
        }

        private string T(string key) { return (string)FindResource(key); }
        private void DragTitle(object sender, MouseButtonEventArgs e) { if (e.LeftButton == MouseButtonState.Pressed) DragMove(); }
        private void MinimizeWindow(object sender, RoutedEventArgs e) { WindowState = WindowState.Minimized; }
        private void CloseWindow(object sender, RoutedEventArgs e) { Close(); }
        private sealed class FolderDialogOwner : Forms.IWin32Window { public IntPtr Handle { get; set; } }
        private void BrowseFolder(object sender, RoutedEventArgs e)
        {
            using (var dialog = new Forms.FolderBrowserDialog { Description = T("Folder"), SelectedPath = Environment.ExpandEnvironmentVariables(Destination.Text.Trim()) })
                if (dialog.ShowDialog(new FolderDialogOwner { Handle = new WindowInteropHelper(this).Handle }) == Forms.DialogResult.OK)
                    Destination.Text = dialog.SelectedPath;
        }

        private void ShowPage(bool guided)
        {
            WelcomePanel.Visibility = guided ? Visibility.Collapsed : Visibility.Visible;
            GuidedPanel.Visibility = guided ? Visibility.Visible : Visibility.Collapsed;
            InstallButton.Visibility = WelcomePanel.Visibility;
            MainButton.Visibility = GuidedPanel.Visibility;
            if (!guided) CloseButton.Visibility = LogButton.Visibility = Visibility.Collapsed;
            var panel = guided ? (UIElement)GuidedPanel : WelcomePanel;
            if (SystemParameters.ClientAreaAnimation)
            {
                panel.Opacity = .4;
                SetupTheme.Move(panel, OpacityProperty, 1, 150);
            }
        }

        private async void StartInstall(object sender, RoutedEventArgs e)
        {
            if (page != Page.Welcome) return;
            try
            {
                string value = Environment.ExpandEnvironmentVariables(Destination.Text.Trim());
                if (!(value.StartsWith(@"\\") || (value.Length >= 3 && value[1] == ':' && (value[2] == '\\' || value[2] == '/'))))
                    throw new ArgumentException();
                destination = FilePath.GetFullPath(value).TrimEnd('\\');
                if (destination.Length <= FilePath.GetPathRoot(value).TrimEnd('\\').Length || File.Exists(destination))
                    throw new ArgumentException();
            }
            catch (Exception) { PathHint.Visibility = Visibility.Visible; Destination.Focus(); return; }

            page = Page.Running;
            Heading.Text = T("Installing"); Status.Text = ""; MainButton.Content = T("Cancel");
            MainButton.Tone = ButtonTone.Secondary;
            AutomationProperties.SetAutomationId(MainButton, "CancelInstallation");
            MainButton.IsEnabled = true;
            CloseButton.Visibility = LogButton.Visibility = Visibility.Collapsed;
            ShowPage(true);
            SetProgress(0, 0);
            try
            {
                log = FilePath.Combine(FilePath.GetTempPath(), "Rikai-Setup-" + Guid.NewGuid().ToString("N") + ".log");
                engine = new InstallEngine();
                var options = arguments.Where(arg => !new[] { "/DIR=", "/LANG=", "/RIKAI_GUIDED=", "/RIKAI_RESTART=", "/LOG=" }
                    .Any(prefix => arg.StartsWith(prefix, StringComparison.OrdinalIgnoreCase)))
                    .Concat(new[] { "/SP-", "/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/NORESTARTAPPLICATIONS",
                        "/RIKAI_GUIDED=1", "/LANG=" + engineLanguages[LocalePicker.SelectedIndex], "/DIR=" + destination, "/LOG=" + log }).ToArray();
                poll.Start();
                ExitCode = await Task.Run(() => engine.Run(options));
                ReadProgress();
                bool cancelled = page == Page.Stopping && (ExitCode == 2 || ExitCode == 5);
                page = ExitCode == 0 ? Page.Finished : cancelled ? Page.Cancelled : Page.Failed;
                Status.Text = page == Page.Failed ? T("ErrorCode") + " " + ExitCode : "";
            }
            catch (Exception ex) { page = Page.Failed; ExitCode = 1; Status.Text = ex.GetBaseException().Message; }
            finally
            {
                poll.Stop();
                if (engine != null) { engine.Dispose(); engine = null; }
            }
            if (page != Page.Failed && File.Exists(log))
                try { File.Delete(log); } catch (IOException) { } catch (UnauthorizedAccessException) { }
            if (page == Page.Cancelled)
            {
                if (closeAfterCancel) { Close(); return; }
                ReturnToFolder();
                return;
            }
            closeAfterCancel = false;
            if (page == Page.Finished) SetProgress(3, 100);
            else DrawSteps();
            Heading.Text = T(page == Page.Finished ? "Done" : "Failed");
            MainButton.IsEnabled = true;
            MainButton.Content = T(page == Page.Finished ? "Open" : "Retry");
            MainButton.Tone = ButtonTone.Primary;
            AutomationProperties.SetAutomationId(MainButton, page == Page.Finished ? "Open" : "Retry");
            CloseButton.Visibility = Visibility.Visible;
            if (page == Page.Failed && File.Exists(log)) LogButton.Visibility = Visibility.Visible;
        }

        private void ReadProgress()
        {
            if (engine == null) return;
            try
            {
                using (var file = new FileStream(engine.ProgressFile, FileMode.Open, FileAccess.Read, FileShare.ReadWrite | FileShare.Delete))
                using (var reader = new StreamReader(file))
                {
                    var parts = reader.ReadToEnd().Trim().Split(' ');
                    int nextPhase, nextPercent;
                    if (parts.Length == 2 && int.TryParse(parts[0], out nextPhase) && int.TryParse(parts[1], out nextPercent)
                            && nextPhase >= 0 && nextPhase <= 2 && nextPercent >= 0 && nextPercent <= 100
                            && (nextPhase != phase || Math.Min(99, nextPercent) != percent))
                        SetProgress(nextPhase, Math.Min(99, nextPercent));
                }
            }
            catch (IOException) { } // A fresh snapshot will be available on the next tick.
        }

        private void SetProgress(int stage, int value)
        {
            if (stage != phase) { phase = stage; DrawSteps(); }
            percent = value;
            Percent.Text = (value / 100d).ToString("P0", culture);
            DrawProgress(true);
        }
        private void DrawProgress(bool animate)
        {
            double target = Track.ActualWidth * percent / 100;
            SetupTheme.Move(Fill, WidthProperty, target, animate ? 170 : 0);
        }

        private void DrawSteps()
        {
            Steps.Children.Clear();
            for (int i = 0; i < stages.Length; i++)
            {
                var row = new StackPanel { Orientation = Orientation.Horizontal, MinHeight = 23, Margin = new Thickness(0,i == 0 ? 0 : 5,0,0) };
                bool done = i < phase, current = i == phase && page == Page.Running;
                var gray = new SolidColorBrush(Color.FromRgb(165, 165, 173));
                var badge = new Grid { Width = 14, Height = 14, Margin = new Thickness(0,0,9,0), VerticalAlignment = VerticalAlignment.Center };
                var complete = new SolidColorBrush(Color.FromRgb(210,210,217));
                badge.Children.Add(new Ellipse { Stroke = done ? complete : new SolidColorBrush(Color.FromRgb(89,89,96)),
                    StrokeThickness = 1, Fill = done ? complete : Brushes.Transparent });
                if (done) badge.Children.Add(new System.Windows.Shapes.Rectangle { Width = 10, Height = 10,
                    Fill = new SolidColorBrush(Color.FromRgb(32,32,36)), OpacityMask = (Brush)FindResource("Glyph.check") });
                if (current)
                {
                    var arc = new System.Windows.Shapes.Path { Data = Geometry.Parse("M7,1 A6,6 0 0 1 13,7"), Stroke = Brushes.White,
                        StrokeThickness = 1.4, RenderTransformOrigin = new Point(.5,.5), RenderTransform = new RotateTransform() };
                    if (SystemParameters.ClientAreaAnimation)
                        arc.RenderTransform.BeginAnimation(RotateTransform.AngleProperty, new DoubleAnimation(0,360,TimeSpan.FromSeconds(.9))
                            { RepeatBehavior = RepeatBehavior.Forever });
                    badge.Children.Add(arc);
                }
                row.Children.Add(badge);
                row.Children.Add(new TextBlock { Text = T(stages[i]), FontSize = 12, VerticalAlignment = VerticalAlignment.Center,
                    Foreground = done || current ? Foreground : gray });
                Steps.Children.Add(row);
            }
        }

        private void ReturnToFolder()
        {
            page = Page.Welcome;
            phase = -1;
            closeAfterCancel = false;
            PathHint.Visibility = Visibility.Collapsed;
            ShowPage(false);
            InstallButton.Focus();
        }

        private void OnClosing(object sender, CancelEventArgs e)
        {
            if (page == Page.Running || page == Page.Stopping) { e.Cancel = true; closeAfterCancel = true; StopInstall(); }
        }
        private void StopInstall()
        {
            if (page != Page.Running || engine == null) return;
            try { engine.Cancel(); }
            catch (IOException error) { Status.Text = error.Message; return; }
            page = Page.Stopping;
            MainButton.IsEnabled = false; Status.Text = T("Stopping");
        }
        private void GuidedAction(object sender, RoutedEventArgs e)
        {
            if (page == Page.Running) { StopInstall(); return; }
            if (page == Page.Finished)
            {
                try { Process.Start(new ProcessStartInfo(FilePath.Combine(destination,"rikai.exe")) { WorkingDirectory = destination }); Close(); }
                catch (Exception ex) { Status.Text = ex.GetBaseException().Message; }
            }
            if (page == Page.Failed) ReturnToFolder();
        }
        private void OpenLog(object sender, RoutedEventArgs e)
        {
            try { Process.Start(new ProcessStartInfo("notepad.exe", InstallEngine.Quote(log)) { UseShellExecute = false }); }
            catch (Exception ex) { Status.Text = ex.GetBaseException().Message; }
        }
    }
}
