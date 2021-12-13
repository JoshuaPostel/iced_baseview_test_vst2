//! Barebones baseview iced plugin


#[macro_use]
extern crate vst;

//use baseview::{Size, WindowHandle, WindowOpenOptions, WindowScalePolicy};
use baseview::{Size, WindowOpenOptions, WindowScalePolicy};
use iced_baseview::{IcedWindow, Settings, WindowHandle, Application, WindowQueue};

use raw_window_handle::{HasRawWindowHandle, RawWindowHandle};

use vst::buffer::AudioBuffer;
use vst::editor::Editor;
use vst::event::{Event, MidiEvent};
use vst::api::Events;
use vst::util::AtomicFloat;
use vst::plugin::{CanDo, Category, Info, Plugin, PluginParameters};

use ringbuf::{Producer, Consumer, RingBuffer};
use std::sync::Mutex;

use log;
use simplelog;

use iced::{
    canvas::{self, Cache, Canvas, Cursor, Geometry, LineCap, Path, Stroke},
    executor, Color, Command, Container, Element, Length, Slider, slider, Row,
    Point, Rectangle, Subscription, Vector, Clipboard, Text, Column,
};


use std::sync::Arc;

const WINDOW_WIDTH: usize = 1024;
const WINDOW_HEIGHT: usize = 512;


#[derive(Debug, Clone)]
enum Message {
   SliderChanged(u32)
}

struct EditorState {
    params: Arc<GainEffectParameters>,
    slider: slider::State,
    slider_value: u32,
    slider_value_string: String,
    midi_consumer: Arc<Mutex<Consumer<[u8; 3]>>>,
    last_note: [u8; 3],
}

// TODO figure out if the struct is needed or just a middleman
struct TestPluginEditor {
    state: Arc<EditorState>,
    window_handle: Option<WindowHandle<<EditorState as Application>::Message>>,
    is_open: bool,
}

// TODO this nested struct is not needed
struct GainEffectParameters {
    amplitude: AtomicFloat,
}

struct TestPlugin {
    params: Arc<GainEffectParameters>,
    editor: Option<TestPluginEditor>,
    // TODO figure out how to update via the Iced Message pattern?
    midi_producer: Producer<[u8; 3]>,
}

impl Default for GainEffectParameters {
    fn default() -> Self {
        Self { amplitude: AtomicFloat::new(0.5) }
    }
}

impl Default for TestPlugin {
    fn default() -> Self {
        let midi_ring = RingBuffer::<[u8; 3]>::new(1_000);
        let (midi_producer, midi_consumer) = midi_ring.split();
        let slider_value = 50;
        let params = Arc::new(GainEffectParameters::default());
        let state = EditorState {
            params: params.clone(),
            slider: slider::State::new(),
            slider_value,
            slider_value_string: format!("{}", slider_value),
            midi_consumer: Arc::new(Mutex::new(midi_consumer)),
            last_note: [0, 0, 0],
        };
        Self { 
            params: params.clone(),
            editor: Some(TestPluginEditor {
                state: Arc::new(state),
                window_handle: None,
                is_open: false,
            }),
            midi_producer,
        }
    }
}

impl Plugin for TestPlugin {
      fn get_info(&self) -> Info {
          log::info!("called get_info");
          Info {
              name: "Iced Gain Effect in Rust".to_string(),
              vendor: "jpostel".to_string(),
              unique_id: 243123073,
              version: 3,
              inputs: 2,
              outputs: 2,
              // This `parameters` bit is important; without it, none of our
              // parameters will be shown!
              parameters: 1,
              category: Category::Effect,
              ..Default::default()
          }
      }

      fn init(&mut self) {
          log::info!("called init");
          let log_folder = ::dirs::home_dir().unwrap().join("tmp");

          //::std::fs::create_dir(log_folder.clone()).expect("create tmp");

          let log_file = ::std::fs::File::create(log_folder.join("IcedBaseviewTest.log")).unwrap();

          let log_config = simplelog::ConfigBuilder::new()
              .set_time_to_local(true)
              .build();

          let _ = simplelog::WriteLogger::init(simplelog::LevelFilter::Info, log_config, log_file);

          log::info!("init 0");
      }

      fn process_events(&mut self, events: &Events) {
          //log::info!("called process_events");
          for e in events.events() {
              match e {
                  Event::Midi(MidiEvent { data, .. }) => {
                      log::info!("got midi event: {:?}", data);
                      self.midi_producer.push(data).unwrap_or(());
                  },
                  _ => (),
              }
          }
      }
      
      fn get_editor(&mut self) -> Option<Box<dyn Editor>> {
          log::info!("called get_editor");
          if let Some(editor) = self.editor.take() {
              Some(Box::new(editor) as Box<dyn Editor>)
          } else {
              None
          }
      }

      // Here is where the bulk of our audio processing code goes.
      fn process(&mut self, buffer: &mut AudioBuffer<f32>) {
          //log::info!("called process");
          // Read the amplitude from the parameter object
          let amplitude = self.params.amplitude.get();
          log::info!("called process with amplitude: {}", amplitude);
          // First, we destructure our audio buffer into an arbitrary number of
          // input and output buffers.  Usually, we'll be dealing with stereo (2 of each)
          // but that might change.
          for (input_buffer, output_buffer) in buffer.zip() {
              // Next, we'll loop through each individual sample so we can apply the amplitude
              // value to it.
              for (input_sample, output_sample) in input_buffer.iter().zip(output_buffer) {
                  *output_sample = *input_sample * amplitude;
              }
          }
      }

      // Return the parameter object. This method can be omitted if the
      // plugin has no parameters.
      fn get_parameter_object(&mut self) -> Arc<dyn PluginParameters> {
          log::info!("called get_parameter_object");
          log::info!("amplitude: {}", self.params.amplitude.get());
          Arc::clone(&self.params) as Arc<dyn PluginParameters>
      }

      fn can_do(&self, can_do: CanDo) -> vst::api::Supported {
          log::info!("called can_do");
          use vst::api::Supported::*;
          use vst::plugin::CanDo::*;

          match can_do {
              SendEvents | SendMidiEvent | ReceiveEvents | ReceiveMidiEvent => Yes,
              _ => Maybe,
          }
      }
}

impl Editor for TestPluginEditor {
      fn position(&self) -> (i32, i32) {
          (0, 0)
      }

      fn size(&self) -> (i32, i32) {
          (WINDOW_WIDTH as i32, WINDOW_HEIGHT as i32)
      }

      fn open(&mut self, parent: *mut ::std::ffi::c_void) -> bool {
          log::info!("Editor open");
          if self.is_open {
              return false;
          }

          self.is_open = true;

          let settings = Settings {
              window: WindowOpenOptions {
                  title: String::from("imgui-baseview demo window"),
                  size: Size::new(WINDOW_WIDTH as f64, WINDOW_HEIGHT as f64),
                  scale: WindowScalePolicy::SystemScaleFactor,
              },
              flags: (self.state.params.clone(), self.state.midi_consumer.clone()),
          };

          let window_handle = IcedWindow::<EditorState>::open_parented(
              &VstParent(parent),
              settings,
              );

          self.window_handle = Some(window_handle);

          true
        }
            fn is_open(&mut self) -> bool {
          log::info!("Editor done");
          self.is_open
      }

      fn close(&mut self) {
          self.is_open = false;
          if let Some(mut window_handle) = self.window_handle.take() {
              //window_handle.close();
              window_handle.close_window();
          }
      }
    }




impl PluginParameters for GainEffectParameters {
    // the `get_parameter` function reads the value of a parameter.
    fn get_parameter(&self, index: i32) -> f32 {
        match index {
            0 => self.amplitude.get(),
            _ => 0.0,
        }
    }

    // the `set_parameter` function sets the value of a parameter.
    fn set_parameter(&self, index: i32, val: f32) {
        #[allow(clippy::single_match)]
        match index {
            0 => self.amplitude.set(val),
            _ => (),
        }
    }

    // This is what will display underneath our control.  We can
    // format it into a string that makes the most since.
    fn get_parameter_text(&self, index: i32) -> String {
        match index {
            0 => format!("{:.2}", (self.amplitude.get() - 0.5) * 2f32),
            _ => "".to_string(),
        }
    }

    // This shows the control's name.
    fn get_parameter_name(&self, index: i32) -> String {
        match index {
            0 => "Amplitude",
            _ => "",
        }
        .to_string()
    }
}



impl Application for EditorState {
    type Executor = executor::Default;
    type Message = Message;
    type Flags = (Arc<GainEffectParameters>, Arc<Mutex<Consumer<[u8; 3]>>>);

    fn new(flags: Self::Flags) -> (Self, Command<Message>) {
            let (params, midi_consumer) = flags;
            let slider_value = 50;
            let state = EditorState { 
                params, 
                slider: slider::State::new(), 
                slider_value,
                slider_value_string: format!("{}", slider_value),
                midi_consumer,
                last_note: [0, 0, 0],
            };
            (state, Command::none())
    }

//    fn title(&self) -> String {
//        String::from("test plugin")
//    }

    fn update(
        &mut self, 
        _window: &mut WindowQueue, 
        message: Self::Message
    ) -> Command<Message> {
        match message {
            Message::SliderChanged(value) => {
                log::info!("slider changed - value: {}", value);
                let new_value = value as f32 / 100.0;
                log::info!("slider changed - new_value: {}", new_value);
                self.slider_value = value;
                self.slider_value_string = format!("{}", new_value);
                self.params.amplitude.set(new_value);
            },
            _ => (),
        }

        Command::none()
    }

//    fn subscription(&self) -> Subscription<Message> {
//        time::every(std::time::Duration::from_millis(500))
//            .map(|_| Message::Tick(chrono::Local::now()))
//    }

    fn view(&mut self) -> Element<Message> {
        let slider = Slider::new(
            &mut self.slider,
            0..=100, 
            self.slider_value,
            Message::SliderChanged);

        let mut midi_events = self.midi_consumer.lock().unwrap();

        // TODO could be dealing with lots of midi_events, not just one
        if let Some(n) = midi_events.pop() {
            log::info!("found midi data: {:?}", n);
            match n[0] {
                // note on
                //144 => *last_note.lock().unwrap() = n,
                144 => self.last_note = n,
                // note off
                //128 => *state.last_note.lock().unwrap() = n,
                _ => (),
            }
          }


        let row = Column::new()
            .push(Text::new(format!("{}", self.slider_value_string)))
            .push(Text::new(format!("{:?}", self.last_note)))
            .push(slider);

        Container::new(row)
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(20)
            .center_x()
            .center_y()
            .into()
    }
}

impl canvas::Program<Message> for EditorState {
    fn draw(&self, bounds: Rectangle, _cursor: Cursor) -> Vec<Geometry> {
        vec![]
    }
}

struct VstParent(*mut ::std::ffi::c_void);

#[cfg(target_os = "macos")]
unsafe impl HasRawWindowHandle for VstParent {
    fn raw_window_handle(&self) -> RawWindowHandle {
        use raw_window_handle::macos::MacOSHandle;

        RawWindowHandle::MacOS(MacOSHandle {
            ns_view: self.0 as *mut ::std::ffi::c_void,
            ..MacOSHandle::empty()
        })
    }
}

#[cfg(target_os = "windows")]
unsafe impl HasRawWindowHandle for VstParent {
    fn raw_window_handle(&self) -> RawWindowHandle {
        use raw_window_handle::windows::WindowsHandle;

        RawWindowHandle::Windows(WindowsHandle {
            hwnd: self.0,
            ..WindowsHandle::empty()
        })
    }
}

#[cfg(target_os = "linux")]
unsafe impl HasRawWindowHandle for VstParent {
    fn raw_window_handle(&self) -> RawWindowHandle {
        use raw_window_handle::unix::XcbHandle;

        RawWindowHandle::Xcb(XcbHandle {
            window: self.0 as u32,
            ..XcbHandle::empty()
        })
    }
}

plugin_main!(TestPlugin);

