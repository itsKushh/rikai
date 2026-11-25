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
        while (!Element<Button>("CloseButton").IsVisible)
        {
            if (timer.Elapsed > TimeSpan.FromMinutes(4)) throw new TimeoutException("Timed out while installing");
            int percent = int.Parse(Element<TextBlock>("Percent").Text.Split(' ')[0], CultureInfo.InvariantCulture);
            if (!observed && percent > 1 && percent < 99)
            {
                observed = true;
                if (cancel) Click("MainButton"); else Snapshot("guided");
            }
            await Task.Delay(40);
        }
        if (!observed) throw new InvalidOperationException("No intermediate progress from the payload");
        var heading = Element<TextBlock>("Heading").Text;
        if (heading != (string)window.FindResource(cancel ? "Cancelled" : "Done"))
            throw new InvalidOperationException("Unexpected outcome: " + heading + " / " + Element<TextBlock>("Status").Text);
    }

    private static async void ExerciseWindow()
    {
        try
        {
            window = Application.Current.MainWindow;
            Snapshot("folder");
            var path = Element<TextBox>("Destination");
            path.Text = "relative";
            Click("InstallButton");
            if (Element<TextBlock>("PathHint").Text != (string)window.FindResource("Invalid"))
                throw new InvalidOperationException("Relative path accepted");
            path.Text = folder;
            Click("InstallButton");
            await AwaitResult(true);
            if (File.Exists(Path.Combine(folder,"rikai.exe"))) throw new InvalidOperationException("Cancellation did not roll back copied files");
            Click("MainButton");
            Click("InstallButton");
            await AwaitResult(false);
            if (!File.Exists(Path.Combine(folder, "worker", "rikai-worker.exe"))) throw new FileNotFoundException("Missing installed worker");
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
