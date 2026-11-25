using System;
using System.ComponentModel;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Reflection;
using System.Threading.Tasks;
using System.Windows;
using System.Windows.Automation;
using System.Windows.Controls;
using System.Windows.Input;
using System.Windows.Interop;
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
        internal int ExitCode { get; private set; } = 2;

        public SetupWindow(string[] args)
        {
            InitializeComponent();
            arguments = args;
            Destination.Text = InstallEngine.DefaultDestination(args);
            string version = Assembly.GetExecutingAssembly().GetName().Version.ToString(3);
            VersionLabel.Text = "Rikai " + version + " · x64";
            GuidedVersion.Text = "Rikai " + version + " · Windows x64";
            MaxWidth = SystemParameters.WorkArea.Width;
            DrawWave();
            Track.SizeChanged += (s, e) => DrawProgress(false);
            poll.Tick += (s, e) => ReadProgress();
            Closing += OnClosing;
        }

        private string T(string key) { return (string)FindResource(key); }
        private void DragTitle(object sender, MouseButtonEventArgs e) { if (e.LeftButton == MouseButtonState.Pressed) DragMove(); }
        private void MinimizeWindow(object sender, RoutedEventArgs e) { WindowState = WindowState.Minimized; }
        private void CloseWindow(object sender, RoutedEventArgs e) { Close(); }
        private sealed class Owner : Forms.IWin32Window { public IntPtr Handle { get; set; } }
        private void BrowseFolder(object sender, RoutedEventArgs e)
        {
            using (var dialog = new Forms.FolderBrowserDialog { Description = T("Folder"), SelectedPath = Destination.Text })
                if (dialog.ShowDialog(new Owner { Handle = new WindowInteropHelper(this).Handle }) == Forms.DialogResult.OK)
                    Destination.Text = dialog.SelectedPath;
        }

        private void ShowPage(bool guided)
        {
            WelcomePanel.Visibility = guided ? Visibility.Collapsed : Visibility.Visible;
            GuidedPanel.Visibility = guided ? Visibility.Visible : Visibility.Collapsed;
            Rect screen = SystemParameters.WorkArea;
            double center = Left + ActualWidth / 2;
            Width = Math.Min(guided ? 860 : 596, screen.Width);
            Left = Math.Max(screen.Left, Math.Min(center - Width / 2, screen.Right - Width));
            var panel = guided ? (UIElement)GuidedPanel : WelcomePanel;
            if (SystemParameters.ClientAreaAnimation)
                panel.BeginAnimation(OpacityProperty, new DoubleAnimation(.2, 1, TimeSpan.FromMilliseconds(260))
                { EasingFunction = new CubicEase { EasingMode = EasingMode.EaseOut } });
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
            catch (Exception) { PathHint.Text = T("Invalid"); Destination.Focus(); return; }

            page = Page.Running;
            Heading.Text = T("Installing"); Status.Text = T("LocalFiles"); MainButton.Content = T("Cancel");
            MainButton.Style = (Style)FindResource(typeof(Button));
            AutomationProperties.SetAutomationId(MainButton, "CancelInstallation");
            MainButton.IsEnabled = true;
            CloseButton.Visibility = LogButton.Visibility = Visibility.Collapsed;
            ShowPage(true);
            SetProgress(0, 0);
            try
            {
                log = FilePath.Combine(FilePath.GetTempPath(), "Rikai-Setup-" + Guid.NewGuid().ToString("N") + ".log");
                engine = new InstallEngine();
                var options = arguments.Where(arg => !new[] { "/DIR=", "/RIKAI_GUIDED=", "/RIKAI_RESTART=", "/LOG=" }
                    .Any(prefix => arg.StartsWith(prefix, StringComparison.OrdinalIgnoreCase)))
                    .Concat(new[] { "/SP-", "/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/NORESTARTAPPLICATIONS",
                        "/RIKAI_GUIDED=1", "/DIR=" + destination, "/LOG=" + log }).ToArray();
                poll.Start();
                ExitCode = await Task.Run(() => engine.Run(options));
                ReadProgress();
                bool cancelled = page == Page.Stopping && (ExitCode == 2 || ExitCode == 5);
                page = ExitCode == 0 ? Page.Finished : cancelled ? Page.Cancelled : Page.Failed;
                Status.Text = page == Page.Finished ? T("Ready") : page == Page.Failed ? T("ErrorCode") + " " + ExitCode : "";
            }
            catch (Exception ex) { page = Page.Failed; ExitCode = 1; Status.Text = ex.GetBaseException().Message; }
            finally
            {
                poll.Stop();
                if (engine != null) { engine.Dispose(); engine = null; }
            }
            if (page == Page.Finished) SetProgress(3, 100);
            else DrawSteps();
            Heading.Text = T(page == Page.Finished ? "Done" : page == Page.Cancelled ? "Cancelled" : "Failed");
            MainButton.IsEnabled = true;
            MainButton.Content = T(page == Page.Finished ? "Open" : "Retry");
            MainButton.Style = (Style)FindResource("Primary");
            AutomationProperties.SetAutomationId(MainButton, page == Page.Finished ? "Open" : "Retry");
            CloseButton.Visibility = Visibility.Visible;
            if (page == Page.Failed && File.Exists(log)) LogButton.Visibility = Visibility.Visible;
            if (page != Page.Failed && File.Exists(log))
                try { File.Delete(log); } catch (IOException) { } catch (UnauthorizedAccessException) { }
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
                            && nextPhase >= 0 && nextPhase <= 2 && nextPercent >= 0 && nextPercent <= 100)
                        SetProgress(nextPhase, Math.Min(99, nextPercent));
                }
            }
            catch (IOException) { } // A fresh snapshot will be available on the next tick.
        }

        private void SetProgress(int stage, int value)
        {
            if (stage != phase) { phase = stage; DrawSteps(); }
            percent = value;
            Percent.Text = value + " %";
            DrawProgress(true);
        }
        private void DrawProgress(bool animate)
        {
            double target = Track.ActualWidth * percent / 100;
            if (animate && SystemParameters.ClientAreaAnimation)
                Fill.BeginAnimation(WidthProperty, new DoubleAnimation(target, TimeSpan.FromMilliseconds(170))
                    { EasingFunction = new CubicEase { EasingMode = EasingMode.EaseOut } });
            else { Fill.BeginAnimation(WidthProperty, null); Fill.Width = target; }
        }

        private void DrawSteps()
        {
            Steps.Children.Clear();
            for (int i = 0; i < stages.Length; i++)
            {
                var row = new StackPanel { Orientation = Orientation.Horizontal, MinHeight = 34 };
                bool done = i < phase, current = i == phase && page == Page.Running;
                var gray = new SolidColorBrush(Color.FromRgb(158, 158, 169));
                var badge = new Grid { Width = 16, Height = 16, Margin = new Thickness(0,0,11,0), VerticalAlignment = VerticalAlignment.Center };
                badge.Children.Add(new Ellipse { Stroke = done ? Brushes.LightGray : gray, StrokeThickness = 1.3,
                    Fill = done ? Brushes.LightGray : Brushes.Transparent });
                if (done) badge.Children.Add(new System.Windows.Shapes.Path { Data = Geometry.Parse("M3,8 L6.5,11.5 L13,4.5"),
                        Stroke = Brushes.Black, StrokeThickness = 1.5 });
                if (current)
                {
                    var arc = new System.Windows.Shapes.Path { Data = Geometry.Parse("M8,1 A7,7 0 0 1 15,8"), Stroke = Brushes.White,
                        StrokeThickness = 2, RenderTransformOrigin = new Point(.5,.5), RenderTransform = new RotateTransform() };
                    if (SystemParameters.ClientAreaAnimation)
                        arc.RenderTransform.BeginAnimation(RotateTransform.AngleProperty, new DoubleAnimation(0,360,TimeSpan.FromSeconds(1.1))
                            { RepeatBehavior = RepeatBehavior.Forever });
                    badge.Children.Add(arc);
                }
                row.Children.Add(badge);
                row.Children.Add(new TextBlock { Text = T(stages[i]), VerticalAlignment = VerticalAlignment.Center,
                    Foreground = done || current ? Foreground : gray });
                Steps.Children.Add(row);
            }
        }

        private void DrawWave()
        {
            var wave = new StreamGeometry();
            using (var ctx = wave.Open())
                for (int x = 0; x <= 330; x += 3)
                {
                    double h = 2 + 14 * Bell(x,84,33) + 47 * Bell(x,149,18) + 19 * Bell(x,208,27) + 10 * Bell(x,278,16);
                    h *= .35 + .65 * Math.Abs(Math.Sin(.11*x) + .2*Math.Sin(.33*x));
                    ctx.BeginFigure(new Point(x,50-h), false, false); ctx.LineTo(new Point(x,50+h), true, false);
                }
            wave.Freeze();
            var brush = new LinearGradientBrush { StartPoint = new Point(0,0), EndPoint = new Point(1,0) };
            brush.GradientStops.Add(new GradientStop(Color.FromRgb(49,49,56),0));
            brush.GradientStops.Add(new GradientStop(Color.FromRgb(211,211,221),.46));
            brush.GradientStops.Add(new GradientStop(Color.FromRgb(49,49,56),1));
            Wave.Children.Add(new System.Windows.Shapes.Path { Data = wave, Stroke = brush, StrokeThickness = 1.4 });
        }
        private static double Bell(int x, double center, double spread) { return Math.Exp(-Math.Pow((x-center)/spread,2)); }

        private void OnClosing(object sender, CancelEventArgs e)
        {
            if (page == Page.Running || page == Page.Stopping) { e.Cancel = true; StopInstall(); }
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
            if (page == Page.Cancelled || page == Page.Failed)
            {
                page = Page.Welcome; phase = -1; PathHint.Text = T("UserOnly");
                ShowPage(false); Destination.Focus();
            }
        }
        private void OpenLog(object sender, RoutedEventArgs e)
        {
            try { Process.Start(new ProcessStartInfo("notepad.exe", InstallEngine.Quote(log)) { UseShellExecute = false }); }
            catch (Exception ex) { Status.Text = ex.GetBaseException().Message; }
        }
    }
}
