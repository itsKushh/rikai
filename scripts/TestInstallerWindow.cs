// Runs the distributed WPF entry point on a real STA dispatcher and drives its controls.
using System;
using System.Diagnostics;
using System.Globalization;
using System.IO;
using System.Reflection;
using System.Threading;
using System.Threading.Tasks;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Media;
using System.Windows.Media.Imaging;
using System.Windows.Threading;

internal static class TestInstallerWindow
{
    private static Window window;
    private static string folder, preview;
    private static Exception failure;

    [STAThread]
    public static int Main(string[] args)
    {
        var installer = Assembly.LoadFrom(args[0]);
        folder = args[1]; preview = args[2];
        Directory.CreateDirectory(preview);
        // Compiled WPF pages reference their own assembly by name.
        AppDomain.CurrentDomain.AssemblyResolve += (sender, request) =>
            new AssemblyName(request.Name).Name == installer.GetName().Name ? installer : null;
        Thread.CurrentThread.CurrentUICulture = CultureInfo.GetCultureInfo("fr-FR");
        Dispatcher.CurrentDispatcher.BeginInvoke(DispatcherPriority.ApplicationIdle, new Action(ExerciseWindow));
        int exit = (int)installer.EntryPoint.Invoke(null, new object[] { new string[0] });
        if (failure != null) { Console.Error.WriteLine(failure); return 1; }
        return exit;
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

    private static async Task CheckButtonMotion(string name, byte hoverRed)
    {
        var button = Element<Button>(name);
        button.ApplyTemplate();
        var surface = (FrameworkElement)button.Template.FindName("Face", button);
        var hover = (Border)button.Template.FindName("HoverFill", button);
        var inset = (Border)button.Template.FindName("Inset", button);
        var shift = (TranslateTransform)surface.RenderTransform;
        if (((SolidColorBrush)hover.Background).Color.R != hoverRed)
            throw new Exception("Wrong hover colour for " + name);

        // Exercise the same WPF pressed state as mouse and keyboard activation.
        var setter = typeof(System.Windows.Controls.Primitives.ButtonBase).GetProperty("IsPressed").GetSetMethod(true);
        setter.Invoke(button, new object[] { true });
        await Task.Delay(180);
        if (Math.Abs(shift.Y - .60) > .01 || inset.Opacity < .99) throw new Exception("Missing pressed state on " + name);
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
        bool observed = false;
        while (!(cancel && Element<Button>("InstallButton").IsVisible) && !Element<Button>("CloseButton").IsVisible)
        {
            if (timer.Elapsed > TimeSpan.FromMinutes(4)) throw new TimeoutException("Timed out while installing");
            int percent = int.Parse(Element<TextBlock>("Percent").Text.Replace("%", "").Trim(), CultureInfo.InvariantCulture);
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
            Snapshot("folder");
            var path = Element<TextBox>("Destination");
            path.Text = "relative";
            Click("InstallButton");
            if (!Element<TextBlock>("PathHint").IsVisible || Element<TextBlock>("PathHint").Text != (string)window.FindResource("Invalid"))
                throw new InvalidOperationException("Relative path accepted");
            path.Text = folder;
            Click("InstallButton");
            await AwaitResult(true);
            if (File.Exists(Path.Combine(folder,"rikai.exe"))) throw new InvalidOperationException("Cancellation did not roll back copied files");
            if (path.Text != folder) throw new Exception("The selected folder was lost after cancellation");
            Click("InstallButton");
            await AwaitResult(false);
            if (!File.Exists(Path.Combine(folder, "worker", "rikai-worker.exe"))) throw new FileNotFoundException("Missing installed worker");
            window.UpdateLayout();
            if (window.ActualWidth != width || window.ActualHeight != height) throw new Exception("Window moved between steps");
            await CheckButtonMotion("CloseButton", 0x2C);
            Snapshot("complete");
            Click("CloseButton");
            Console.WriteLine("Guided install, real progress, cancellation, rollback, retry and close verified.");
        }
        catch (Exception error)
        {
            failure = error;
            Application.Current.Shutdown(1);
        }
    }
}
