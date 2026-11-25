// Runs the distributed WPF entry point on a real STA dispatcher and drives its controls.
using System;
using System.Diagnostics;
using System.Globalization;
using System.IO;
using System.Reflection;
using System.Runtime.InteropServices;
using System.Threading;
using System.Threading.Tasks;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Interop;
using System.Windows.Media;
using System.Windows.Media.Imaging;
using System.Windows.Threading;

internal static class TestInstallerWindow
{
    private static Window window;
    private static string folder, preview;
    private static Exception failure;
    private static bool animations;

    [DllImport("user32.dll")] private static extern bool SystemParametersInfo(uint action, uint param, ref int value, uint flags);
    [DllImport("user32.dll")] private static extern bool SystemParametersInfo(uint action, uint param, IntPtr value, uint flags);
    [DllImport("user32.dll")] private static extern IntPtr SetCapture(IntPtr windowHandle);
    [DllImport("user32.dll")] private static extern bool ReleaseCapture();
    [DllImport("user32.dll")] private static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] private static extern IntPtr SendMessage(IntPtr windowHandle, uint message, IntPtr wParam, IntPtr lParam);

    [STAThread]
    public static int Main(string[] args)
    {
        var installer = Assembly.LoadFrom(args[0]);
        folder = args[1]; preview = args[2];
        Directory.CreateDirectory(preview);
        // Compiled WPF pages reference their own assembly by name.
        AppDomain.CurrentDomain.AssemblyResolve += (sender, request) =>
            new AssemblyName(request.Name).Name == installer.GetName().Name ? installer : null;
        // Set the disposable runner's session preference before WPF caches it.
        animations = args[3] == "animated";
        int original = 0;
        if (!SystemParametersInfo(0x1042, 0, ref original, 0)) throw new Exception("Cannot read Windows animation preference");
        try
        {
            if (!SystemParametersInfo(0x1043, 0, new IntPtr(animations ? 1 : 0), 0)) throw new Exception("Cannot set Windows animation preference");
            Thread.CurrentThread.CurrentUICulture = CultureInfo.GetCultureInfo("fr-FR");
            Dispatcher.CurrentDispatcher.BeginInvoke(DispatcherPriority.ApplicationIdle, new Action(ExerciseWindow));
            int exit = (int)installer.EntryPoint.Invoke(null, new object[] { new string[0] });
            if (failure != null) { Console.Error.WriteLine(failure); return 1; }
            return exit;
        }
        finally { SystemParametersInfo(0x1043, 0, new IntPtr(original), 0); }
    }

    private static T Element<T>(string name) where T : FrameworkElement
    {
        var control = window.FindName(name) as T;
        if (control == null) throw new InvalidOperationException("Missing control: " + name);
        return control;
    }

    private static void Click(string name)
    {
        var button = Element<Button>(name);
        if (!button.IsEnabled || !button.IsVisible) throw new InvalidOperationException("Unavailable button: " + name);
        button.RaiseEvent(new RoutedEventArgs(Button.ClickEvent));
    }

    private static void MovePointer(FrameworkElement target, Point local)
    {
        var source = (HwndSource)PresentationSource.FromVisual(window);
        var client = source.CompositionTarget.TransformToDevice.Transform(target.TranslatePoint(local, window));
        var screen = target.PointToScreen(local);
        SetCursorPos((int)screen.X, (int)screen.Y);
        // The CI window can be on a noninteractive desktop. Capture allows WPF to
        // accept WM_MOUSEMOVE and perform its normal hit testing there as well.
        SetCapture(source.Handle);
        SendMessage(source.Handle, 0x0200, IntPtr.Zero, new IntPtr(((int)client.Y << 16) | (int)client.X));
    }

    private static async Task CheckButtonMotion(string name, byte? hoverRed)
    {
        var button = Element<Button>(name);
        button.ApplyTemplate();
        var surface = (FrameworkElement)button.Template.FindName("Face", button);
        var hover = (Border)button.Template.FindName("HoverFill", button);
        var inset = (Border)button.Template.FindName("Inset", button);
        var shift = (TranslateTransform)surface.RenderTransform;
        bool caption = !hoverRed.HasValue;
        if (!caption && ((SolidColorBrush)hover.Background).Color.R != hoverRed)
            throw new Exception("Wrong hover colour for " + name);

        var center = new Point(button.ActualWidth / 2, button.ActualHeight / 2);
        var outside = new Point(6, 60);
        var feedback = caption ? surface : hover;
        MovePointer(window, outside);
        MovePointer(button, center);
        await Task.Delay(240);
        if (!button.IsMouseOver || feedback.Opacity < .99) throw new Exception("Hover did not persist on " + name);
        MovePointer(window, outside);
        await Task.Delay(35);
        MovePointer(button, center);
        await Task.Delay(240);
        if (!button.IsMouseOver || feedback.Opacity < .99) throw new Exception("Hover reversal failed on " + name);
        MovePointer(window, outside);
        await Task.Delay(240);
        ReleaseCapture();
        if (button.IsMouseOver || Math.Abs(feedback.Opacity - (caption ? .60 : 0)) > .01)
            throw new Exception("Hover did not clear on " + name);

        // Exercise the same WPF pressed state as mouse and keyboard activation.
        var setter = typeof(System.Windows.Controls.Primitives.ButtonBase).GetProperty("IsPressed").GetSetMethod(true);
        setter.Invoke(button, new object[] { true });
        await Task.Delay(180);
        if (Math.Abs(shift.Y - .60) > .01 || Math.Abs(inset.Opacity - (caption ? 0 : 1)) > .01)
            throw new Exception("Missing pressed state on " + name);
        setter.Invoke(button, new object[] { false });
        await Task.Delay(180);
        if (Math.Abs(shift.Y) > .01 || inset.Opacity > .01) throw new Exception("Pressed state did not clear on " + name);
    }

    private static void Snapshot(string name)
    {
        window.UpdateLayout();
        var bitmap = new RenderTargetBitmap((int)Math.Ceiling(window.ActualWidth), (int)Math.Ceiling(window.ActualHeight), 96, 96, PixelFormats.Pbgra32);
        bitmap.Render(window);
        var png = new PngBitmapEncoder();
        png.Frames.Add(BitmapFrame.Create(bitmap));
        using (var stream = File.Create(Path.Combine(preview, name + ".png"))) png.Save(stream);
    }

    private static async Task AwaitResult(bool cancel)
    {
        var timer = Stopwatch.StartNew();
        var unchanged = Stopwatch.StartNew();
        int lastPercent = -1;
        bool observed = false;
        while (!(cancel && Element<Button>("InstallButton").IsVisible) && !Element<Button>("CloseButton").IsVisible)
        {
            if (timer.Elapsed > TimeSpan.FromMinutes(4)) throw new TimeoutException("Timed out while installing");
            int percent = int.Parse(Element<TextBlock>("Percent").Text.Replace("%", "").Trim(), CultureInfo.InvariantCulture);
            if (percent != lastPercent) { lastPercent = percent; unchanged.Restart(); }
            if (Element<StackPanel>("GuidedPanel").Opacity < .99) throw new Exception("Guide dimmed during installation");
            var fill = Element<Border>("Fill");
            if (percent > 0 && fill.Width < 1) throw new Exception("Missing progress fill at " + percent + "%");
            if (percent > 0 && unchanged.ElapsedMilliseconds > 300 &&
                Math.Abs(fill.ActualWidth - Element<Border>("Track").ActualWidth * percent / 100) > 1)
                throw new Exception("Progress fill did not hold at " + percent + "%");
            if (!observed && percent >= (cancel ? 3 : 45) && percent < 99)
            {
                observed = true;
                if (cancel) Click("MainButton"); else Snapshot("guided");
            }
            await Task.Delay(40);
        }
        if (!observed) throw new InvalidOperationException("No intermediate progress from the payload");
        if (cancel && Element<Button>("InstallButton").IsVisible) return;
        var heading = Element<TextBlock>("Heading").Text;
        if (cancel || heading != (string)window.FindResource("Done"))
            throw new InvalidOperationException("Unexpected outcome: " + heading + " / " + Element<TextBlock>("Status").Text);
    }

    private static async void ExerciseWindow()
    {
        try
        {
            window = Application.Current.MainWindow;
            if (SystemParameters.ClientAreaAnimation != animations) throw new Exception("Wrong motion preference in WPF");
            var languages = Element<ComboBox>("LocalePicker");
            if (languages.SelectedIndex != 0 || languages.Items.Count != 6) throw new Exception("System language was not selected");
            var destination = Element<TextBox>("Destination").Text;
            string[] titles = { "Installation de Rikai", "Rikai setup", "Instalación de Rikai", "Установка Rikai", "Rikai のセットアップ", "安装 Rikai" };
            for (int i = 0; i < titles.Length; i++)
            {
                languages.SelectedIndex = i;
                if (window.Title != titles[i] || Element<TextBox>("Destination").Text != destination)
                    throw new Exception("Language selection did not update the window in place");
            }
            languages.SelectedIndex = 0;
            window.UpdateLayout();
            double width = window.ActualWidth, height = window.ActualHeight;
            if (width != 392 || Math.Abs(height - 316) > 2) throw new Exception("Unexpected size: " + width + " x " + height);
            await CheckButtonMotion("InstallButton", 0xCA);
            await CheckButtonMotion("MinimizeButton", null);
            await CheckButtonMotion("TitleCloseButton", null);
            Snapshot("folder");
            var path = Element<TextBox>("Destination");
            path.Text = "relative";
            Click("InstallButton");
            if (!Element<TextBlock>("PathHint").IsVisible || Element<TextBlock>("PathHint").Text != (string)window.FindResource("Invalid"))
                throw new InvalidOperationException("Relative path accepted");
            path.Text = folder;
            Click("InstallButton");
            window.UpdateLayout();
            if (Element<Border>("Fill").ActualWidth < 1 || Element<StackPanel>("GuidedPanel").Opacity < .99)
                throw new Exception("Preparation indicator or guide missing on entry");
            await CheckButtonMotion("MainButton", 0x36);
            await AwaitResult(true);
            if (File.Exists(Path.Combine(folder,"rikai.exe"))) throw new InvalidOperationException("Cancellation did not roll back copied files");
            if (path.Text != folder) throw new Exception("The selected folder was lost after cancellation");
            Click("InstallButton");
            await AwaitResult(false);
            if (!File.Exists(Path.Combine(folder, "worker", "rikai-worker.exe"))) throw new FileNotFoundException("Missing installed worker");
            window.UpdateLayout();
            if (window.ActualWidth != width || window.ActualHeight != height) throw new Exception("Window moved between steps");
            if (Math.Abs(Element<Border>("Track").ActualWidth - Element<Border>("Fill").ActualWidth) > 1)
                throw new Exception("Incomplete final progress bar");
            await CheckButtonMotion("CloseButton", 0x2C);
            Snapshot("complete");
            Click("CloseButton");
            Console.WriteLine("Hover, reversal, press, caption buttons, visible guide, real progress, cancellation, rollback and retry verified (" + (animations ? "animations on" : "reduced motion") + ").");
        }
        catch (Exception error)
        {
            failure = error;
            Application.Current.Shutdown(1);
        }
    }
}
