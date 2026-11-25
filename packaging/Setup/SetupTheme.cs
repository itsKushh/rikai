using System;
using System.Globalization;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Media;
using System.Windows.Media.Animation;
using System.Xml;

namespace Rikai.Setup
{
    public enum ButtonTone { Secondary, Primary, Quiet, Caption }

    // Matches glide(t) in the desktop controls. The Bézier x coordinates are 1/3 and 2/3.
    internal sealed class RikaiEase : EasingFunctionBase
    {
        protected override double EaseInCore(double t)
        {
            double u = 1 - t;
            return 3 * u * u * t * .78 + 3 * u * t * t + t * t * t;
        }
        protected override Freezable CreateInstanceCore() { return new RikaiEase(); }
    }

    internal static class SetupTheme
    {
        private static readonly RikaiEase glide = new RikaiEase { EasingMode = EasingMode.EaseIn };

        // A reversal starts from the displayed value, including rapid pointer re-entry.
        internal static void Move(DependencyObject element, DependencyProperty property, double to, int ms)
        {
            var animatable = (IAnimatable)element;
            double from = (double)element.GetValue(property);
            if ((double)animatable.GetAnimationBaseValue(property) == to && (ms > 0 || from == to)) return;
            element.SetCurrentValue(property, to);
            var animation = SystemParameters.ClientAreaAnimation && ms > 0 && from != to
                ? new DoubleAnimation(from, to, TimeSpan.FromMilliseconds(ms))
                    { EasingFunction = glide, FillBehavior = FillBehavior.Stop } : null;
            animatable.BeginAnimation(property, animation);
        }

        internal static void LoadIcons(Application app)
        {
            foreach (string name in new[] { "minus", "xmark", "language", "chevron-down", "folder-open", "check" })
            {
                var svg = new XmlDocument { XmlResolver = null };
                using (var stream = typeof(SetupTheme).Assembly.GetManifestResourceStream("Rikai.Icon." + name + ".svg"))
                    svg.Load(stream);
                var group = new DrawingGroup();
                foreach (XmlElement path in svg.GetElementsByTagName("path"))
                {
                    string opacity = path.HasAttribute("fill-opacity") ? path.GetAttribute("fill-opacity") : path.GetAttribute("opacity");
                    var ink = new SolidColorBrush(Colors.White) { Opacity = opacity == "" ? 1 : double.Parse(opacity, CultureInfo.InvariantCulture) };
                    var geometry = Geometry.Parse((path.GetAttribute("fill-rule") == "evenodd" ? "F0 " : "F1 ") + path.GetAttribute("d"));
                    group.Children.Add(new GeometryDrawing(ink, null, geometry));
                }
                var mask = new DrawingBrush(group) { ViewboxUnits = BrushMappingMode.Absolute, Viewbox = new Rect(0,0,18,18), Stretch = Stretch.Uniform };
                mask.Freeze();
                app.Resources["Glyph." + name] = mask;
            }
        }
    }

    public class SetupButton : Button
    {
        public static readonly DependencyProperty ToneProperty = DependencyProperty.Register("Tone", typeof(ButtonTone),
            typeof(SetupButton), new PropertyMetadata(ButtonTone.Secondary));
        public static readonly DependencyProperty RadiusProperty = DependencyProperty.Register("Radius", typeof(CornerRadius),
            typeof(SetupButton), new PropertyMetadata(new CornerRadius(11)));
        public ButtonTone Tone { get { return (ButtonTone)GetValue(ToneProperty); } set { SetValue(ToneProperty, value); } }
        public CornerRadius Radius { get { return (CornerRadius)GetValue(RadiusProperty); } set { SetValue(RadiusProperty, value); } }

        private FrameworkElement face, hover, pressed, focus;

        public override void OnApplyTemplate()
        {
            base.OnApplyTemplate();
            face = (FrameworkElement)GetTemplateChild("Face");
            hover = (FrameworkElement)GetTemplateChild("HoverFill");
            pressed = (FrameworkElement)GetTemplateChild("Inset");
            focus = (FrameworkElement)GetTemplateChild("FocusRing");
            if (face != null) face.RenderTransform = new TranslateTransform();
            Refresh(false);
        }
        protected override void OnPropertyChanged(DependencyPropertyChangedEventArgs e)
        {
            base.OnPropertyChanged(e);
            if (e.Property == IsMouseOverProperty || e.Property == IsKeyboardFocusedProperty ||
                e.Property == IsPressedProperty || e.Property == IsEnabledProperty || e.Property == ToneProperty)
                Refresh(true);
        }
        private void Refresh(bool animate)
        {
            if (face == null || hover == null || pressed == null || focus == null) return;
            bool caption = Tone == ButtonTone.Caption;
            bool over = IsEnabled && IsMouseOver;
            bool down = IsEnabled && IsPressed && !caption;
            bool move = animate && IsEnabled;
            SetupTheme.Move(face, OpacityProperty, !IsEnabled ? .48 : caption && !over && !IsKeyboardFocused ? .60 : 1, move ? 115 : 0);
            SetupTheme.Move(hover, OpacityProperty, over && !caption ? 1 : 0, move ? 120 : 0);
            SetupTheme.Move(pressed, OpacityProperty, down ? 1 : 0, move ? 90 : 0);
            SetupTheme.Move(face.RenderTransform, TranslateTransform.YProperty, down ? .60 : 0, move ? 90 : 0);
            SetupTheme.Move(focus, OpacityProperty, IsEnabled && IsKeyboardFocused && System.Windows.Input.InputManager.Current.MostRecentInputDevice is System.Windows.Input.KeyboardDevice ? 1 : 0, move ? 100 : 0);
        }
    }
}
