mod daily;
mod alert;
mod hourly;

use std::sync::{Arc, Mutex, Weak};
use std::path::{Path, PathBuf};
use std::env::current_dir;
use core::future::Future;

use gtk::prelude::*;
use gtk::{
    ApplicationWindow,
    ActionBar,
    CenterBox,
    Label,
    EditableLabel,
    Picture,
    Popover,
    Image,
    Entry,
    Button,
    Switch,
    Stack,
    ComboBoxText,
    ListStore,
    MenuButton,
    Widget,
};
use flume::Sender;
use super::preferences::WeatherPreferences;
use super::api::{
    weather::*,
    location::*,
    units::Units,
};
use alert::WeatherAlerts;
use daily::DailyView;
use hourly::HourlyView;
use super::rpc::WeatherUpdate;

pub struct WeatherApplication {
    active: bool,
    sender: Option<Sender<WeatherUpdate>>,
    mutex: Option<Weak<Mutex<Self>>>,
    units_switch: Switch,
    location: EditableLabel,
    location_search: Entry,
    location_search_button: Button,
    location_results: ComboBoxText,
    refresh_button: Button,
    temperature: Label,
    feels_like: Label,
    current_details: Label,
    current_picture: Picture,
    alerts: WeatherAlerts,
    daily: DailyView,
    hourly: HourlyView,
    stack_view: Arc<Mutex<Stack>>,
    stack_buttons_container: CenterBox,
    preferences: Option<WeatherPreferences>,
}

pub fn icon_path(icon: Option<String>) -> PathBuf {
    let pwd = current_dir().unwrap();
    let path = if let Some(icon) = icon {
        format!("{}/icons/{}.png", pwd.display(), &icon)
    } else {
        format!("{}/icons/unknown.png", pwd.display())
    };
    Path::new(&path).to_path_buf()
}

fn current_picture_path(current: Option<&CurrentWeather>) -> PathBuf {
    let path = if current.is_some() && current.unwrap().status.len() > 0 {
        icon_path(Some(current.unwrap().status[0].icon.clone()))
    } else {
        icon_path(None)
    };

    Path::new(&path).to_path_buf()
}

impl WeatherApplication {
    pub fn new(window: &ApplicationWindow) -> Self {
        let temperature = Label::new(None);
        let feels_like = Label::new(None);
        let location = EditableLabel::new("");
        location.set_visible(false);

        let location_image = Image::from_icon_name(Some("mark-location"));

        let location_search = Entry::new();
        let location_search_button = Button::from_icon_name(Some("edit-find"));
        let location_results = ComboBoxText::new();
        location_results.set_visible(false);
        location_results.set_id_column(0);
        
        let refresh_button = Button::from_icon_name(Some("view-refresh"));
        refresh_button.set_visible(false);

        let location_box = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        location_search.set_placeholder_text(Some("Search your location..."));
        location_box.append(&location_image);
        location_box.append(&location);
        location_box.append(&location_search);
        location_box.append(&location_results);
        location_box.append(&location_search_button);
        location_box.append(&refresh_button);

        let action_bar = ActionBar::new();
        action_bar.set_center_widget(Some(&location_box));
        
        let preferences_container = gtk::Box::new(gtk::Orientation::Vertical, 10);
        let preferences_title = Label::new(None);
        preferences_title.set_markup("<b>Units</b>");
        preferences_container.append(&preferences_title);

        let units_container = gtk::Box::new(gtk::Orientation::Horizontal, 5);
        let units_switch = Switch::new();
        units_container.append(&Label::new(Some("Imperial")));
        units_container.append(&units_switch);
        units_container.append(&Label::new(Some("Metric")));
        preferences_container.append(&units_container);

        let preferences_popover = Popover::new();
        preferences_popover.set_child(Some(&preferences_container));
        preferences_popover.set_autohide(true);
        
        let preferences_menu = MenuButton::new();
        preferences_menu.set_icon_name("preferences-system");
        preferences_menu.set_popover(Some(&preferences_popover));
        action_bar.pack_end(&preferences_menu);

        let current_picture = Picture::new();
        let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        hbox.append(&current_picture);
        hbox.append(&temperature);

        let chbox = CenterBox::new();
        chbox.set_center_widget(Some(&hbox));


        let current_details = Label::new(None);

        let alerts_container = CenterBox::new();
        let alerts = WeatherAlerts::new(None);
        alerts_container.set_center_widget(Some(&alerts.container));

        let daily = DailyView::new();
        daily.set_visible(false);
        let daily_container = CenterBox::new();
        daily_container.set_center_widget(Some(&daily.container));
        
        let hourly = HourlyView::new();
        hourly.set_visible(false);
        let hourly_container = CenterBox::new();
        hourly_container.set_center_widget(Some(&hourly.container));

        let stack = Stack::new();
        stack.set_vhomogeneous(false);
        stack.set_interpolate_size(true);

        let stack_pages = vec![
            stack.add_titled(&alerts_container, Some("alerts"), "Alerts"),
            stack.add_titled(&current_details, Some("current"), "Currently"),
            stack.add_titled(&hourly_container, Some("hourly"), "Hourly"),
            stack.add_titled(&daily_container, Some("daily"), "Weekly"),
        ];
        let stack_buttons = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        let stack_view = &Arc::new(Mutex::new(stack));
        for stack_page in stack_pages.iter() {
            let stack_button = Button::new();
            if let Some(title) = stack_page.title() {
                stack_button.set_label(&title);
            }
            let stack_view_arc = stack_view.clone();
            let name = stack_page.name().clone().unwrap();
            stack_button.connect_clicked(move |_| {
                if let Ok(stack_view) = stack_view_arc.try_lock() {
                    if let Some(page) = stack_view.child_by_name(&name) {
                        stack_view.set_visible_child(&page);
                    }
                }
            });
            stack_buttons.append(&stack_button);
        }
        let stack_buttons_container = CenterBox::new();
        stack_buttons_container.set_center_widget(Some(&stack_buttons));

        let vbox = gtk::Box::new(gtk::Orientation::Vertical, 10);
        vbox.append(&action_bar);
        vbox.append(&chbox);
        vbox.append(&feels_like);
        vbox.append(&stack_buttons_container);

        if let Ok(stack_view) = stack_view.try_lock() {
            let widget = &stack_view.clone().upcast::<Widget>();
            vbox.append(widget);
        }

        window.set_child(Some(&vbox));

        let wa = WeatherApplication {
            temperature,
            location,
            location_search,
            location_search_button,
            location_results,
            refresh_button,
            feels_like,
            current_picture,
            current_details,
            units_switch,
            alerts,
            daily,
            hourly,
            stack_view: stack_view.clone(),
            stack_buttons_container,
            active: true,
            sender: None, 
            mutex: None,
            preferences: None,
        };
    
        wa
    }

    fn spawn_local<Fs: 'static + Future<Output = ()>>(&self, sender_future: Fs) {
        gtk::glib::MainContext::default().spawn_local(sender_future);
    }

    fn get_sender(&self) -> Sender<WeatherUpdate> {
        self.sender.clone()
            .expect("Unable to find application sender")
    }

    pub fn get_mutex(&self) -> Weak<Mutex<Self>> {
        self.mutex.clone().unwrap()
    }
     
    pub fn load(&mut self,
        preferences: Option<WeatherPreferences>,
        sender: Sender<WeatherUpdate>,
        mutex: Weak<Mutex<Self>>) {

        self.sender = Some(sender);
        self.preferences = preferences;
        
        // Bind signals
        if let Some(preferences) = &self.preferences {
            let units_state = match preferences.units {
                Units::Metric => true,
                Units::Imperial => false,
            };
            self.units_switch.set_state(units_state);
        }

        let mutex_units = mutex.clone();
        self.units_switch.connect_state_notify(move |switch| {
            let metric = switch.state();
            let units = match metric {
                true => Units::Metric,
                false => Units::Imperial,
            };
            if let Ok(app) = mutex_units.upgrade().unwrap().try_lock() {
                if let Err(err) = app.get_sender().send(WeatherUpdate::SetUnits(units)) {
                    println!("Unable to update units!");
                    panic!("{}", err);
                }
                if let Err(err) = app.get_sender().send(WeatherUpdate::Refresh) {
                    println!("Unable to refresh weather after units changed: {}", err);
                }
            }
        });

        let mutex_location = mutex.clone();
        self.location.connect_editing_notify(move |l| {
            if !l.is_editing() {
                return;
            }
            if let Ok(app) = mutex_location.upgrade().unwrap().try_lock() {
                app.get_sender().send(WeatherUpdate::Location(None))
                    .expect("Unable to send WeatherUpdate(None) for location");
            }
        });
        
        let mutex_location_search = mutex.clone();
        self.location_search_button.connect_clicked(move |_| { 
            if let Ok(app) = mutex_location_search.upgrade().unwrap().try_lock() {
                let search_query = app.location_search.text();
                if search_query.len() == 0 {
                    return;
                }
                let search_query: &str = &search_query;
                app.get_sender().send(WeatherUpdate::SearchLocations(search_query.to_string()))
                    .expect("Unable to send WeatherUpdate::SearchLocations(None) for Search");
            } else {
                println!("Unable to lock mutex_location");
            }
        });

        let mutex_combo = mutex.clone();
        self.location_results.connect_changed(move |combo| {
            if let Some(active_iter) = combo.active_iter() {
                if let Some(model) = combo.model() {
                    let location = model
                        .get(&active_iter, 0).get::<String>()
                        .expect("location from model at col 0 is String");
                    let lat = model
                        .get(&active_iter, 1).get::<f64>()
                        .expect("lat from model at col 1 is F64");
                    let lon = model
                        .get(&active_iter, 2).get::<f64>()
                        .expect("lon from model at col 2 is F64");

                    let interest = LocationPoint {
                        location,
                        lat,
                        lon,
                    };
                    if let Ok(mut app) = mutex_combo.upgrade().unwrap().try_lock() {
                        if app.preferences.is_some() {
                            app.preferences
                                .as_mut().unwrap()
                                .set_from_location_point(&interest)
                                .save_config();
                        }
                        app.request_weather(interest);
                    }
                }
            }
        });

        let mutex_refresh = mutex.clone();
        self.refresh_button.connect_clicked(move |_| {
            if let Ok(app) = mutex_refresh.upgrade().unwrap().try_lock() {
                if let Err(err) = app.get_sender().send(WeatherUpdate::Refresh) {
                    println!("Unable to refresh weather: {}", err);
                    let _ = app.get_sender().send(WeatherUpdate::Location(None));
                }
            }
        });

        // must be set before request_weather
        self.mutex = Some(mutex);

        // Load current weather if preferences set
        if let Some(preferences) = &self.preferences {
            self.request_weather(LocationPoint {
                location: preferences.location.clone(),
                lat: preferences.lat,
                lon: preferences.lon,
            });
        } else {
            // No preferences set! Set ui state as no-location
            if let Ok(app) = self.get_mutex().clone().upgrade().unwrap().try_lock() {
                if let Err(_) = app.get_sender().send(WeatherUpdate::Location(None)) {
                    println!("Unable to reset location when preferences were not set");
                }
            }
        }

    }

    fn refresh_weather(&self) {
        if let Some(prefs) = &self.preferences {
            self.request_weather(LocationPoint {
                location: prefs.location.clone(),
                lat: prefs.lat,
                lon: prefs.lon,
            });
        }
    }

    fn request_weather(&self, interest: LocationPoint) {
        let mutex = self.get_mutex().clone();

        self.spawn_local(async move {
            if let Ok(app) = mutex.upgrade().unwrap().try_lock() {
                let sender = app.get_sender();

                let new_prefs = WeatherPreferences {
                    location: interest.location,
                    lat: interest.lat,
                    lon: interest.lon,
                    units: app.get_units(),
                };
                let data = get_weather_data(
                   app.get_units(),
                   interest.lat, 
                   interest.lon,
                ).await;

                sender.send_async(WeatherUpdate::Data(data)).await.unwrap();
                sender.send_async(WeatherUpdate::Location(Some(new_prefs.location.clone()))).await.unwrap();
                if let Err(err) = sender.send_async(WeatherUpdate::SavePreferences(new_prefs)).await {
                    println!("Unable to save preferences: {}", err);
                }
            }
        });
    }

    pub fn update(&mut self, update: WeatherUpdate) {
        match update {
            WeatherUpdate::Data(data) => self.update_weather(data),
            WeatherUpdate::Location(location) => self.update_location(location),
            WeatherUpdate::SearchLocations(query) => self.search_location(query),
            WeatherUpdate::SetLocations(locations) => self.update_location_results(locations),
            WeatherUpdate::SavePreferences(preferences) => self.save_preferences(&preferences),
            WeatherUpdate::SetUnits(units) => self.update_units(units),
            WeatherUpdate::Refresh => self.refresh_weather(),
        }
    }
    
    pub fn is_active(&self) -> bool {
        self.active
    }

    fn update_daily_weather(&mut self, daily: Option<Vec<DailyWeather>>) {
        if let Some(daily) = daily {
            self.daily.populate(daily, &self.get_units());
            self.daily.set_visible(true);
        } else {
            self.daily.populate(Vec::new(), &self.get_units());
            self.daily.set_visible(false);
        }
    }

    fn update_hourly_weather(&mut self, hourly: Option<Vec<CurrentWeather>>) {
        if let Some(hourly) = hourly {
            self.hourly.populate(hourly, &self.get_units());
            self.hourly.set_visible(true);
        } else {
            self.hourly.populate(Vec::new(), &self.get_units());
            self.hourly.set_visible(false);
        }
    }

    fn update_current_image(&mut self, current: Option<CurrentWeather>) {
        let picture_path = current_picture_path(current.as_ref());
        self.current_picture.set_filename(picture_path.to_str().unwrap());
    }

    fn update_current_weather(&mut self, current: Option<CurrentWeather>) {
        if let Some(current) = current {
            let units = self.get_units();
            self.temperature.set_markup(&format!("<big>{}</big>", units.temperature_value(current.temp)));
            self.feels_like.set_markup(&format!("<big>Feels like: {}</big>", units.temperature_value(current.feels_like)));
            self.current_details.set_markup(&format!("
<b>At</b> {}
Pressure: {}
Humidity: {}
UV Index: {}
Visibility: {}
Wind Speed: {}
Precipitation: {}%
            ", 
            current.time("[hour]:[minute]"), 
            current.pressure, 
            current.humidity,
            current.uvi,
            current.visibility.unwrap_or(0),
            units.speed_value(current.wind_speed),
            current.pop * 100.00));
            self.update_current_image(Some(current));
            
        } else {
            self.temperature.set_markup("<big>Invalid Data</big>");
            self.feels_like.set_markup("Please try another city name!");
            self.update_current_image(None);
        };
        
    }
    
    fn update_alerts(&mut self, weather_alerts: Option<Vec<WeatherAlert>>) {
        if let Some(weather_alerts) = weather_alerts {
            self.alerts.populate(weather_alerts);
        } else {
            self.alerts.populate(Vec::new());
        }
    }

    fn update_weather(&mut self, weather: Option<WeatherData>) {
        if let Some(weather) = weather {
            let units = weather.units.expect("units");
            self.update_units(units);
            self.update_current_weather(Some(weather.current));
            self.update_daily_weather(Some(weather.daily));
            self.update_hourly_weather(Some(weather.hourly));
            self.update_alerts(Some(weather.alerts));
        } else {
            self.update_current_weather(None);
            self.update_daily_weather(None);
            self.update_hourly_weather(None);
            self.update_alerts(None);
        };
    }

    fn search_location(&self, search_query: String) {
        let search_query = search_query.clone();
        if search_query.len() == 0 {
            return;
        }
        
        let mutex = self.get_mutex();

        self.spawn_local(async move {
            match mutex.upgrade().unwrap().try_lock() {
                Ok(app) => {
                    app.location.set_visible(false);
                    app.location_search.set_visible(false); 
                    app.location_search_button.set_visible(false);

                    let sender = app.get_sender();
                    let locations = search_locations(&search_query).await;
                    if let Err(_) = sender.send_async(WeatherUpdate::SetLocations(locations)).await {
                        println!("Unable to send WeatherUpdate::SetLocations");
                    }
                }, 
                Err(err) => println!("search_location err: {}", err),
            }
        });
    }

    fn locations_to_store(locations: Vec<LocationPoint>) -> ListStore {
        let col_types: [gtk::glib::Type; 3] = [
            gtk::glib::Type::STRING, 
            gtk::glib::Type::F64,
            gtk::glib::Type::F64,
        ];
        let model = ListStore::new(&col_types);

        for l in locations.iter() {
            let columns_values: &[(u32, &dyn ToValue)] = &[(0, &l.location), (1, &l.lat), (2, &l.lon)];
            model.set(&model.append(), &columns_values);
        }

        model
    }

    fn update_location_results(&mut self, location_results: Option<Vec<LocationPoint>>) {
        if let Some(location_results) = location_results {
            let results_count = location_results.len();
            let first_result = if results_count == 1 {
                Some(location_results[0].clone())
            } else {
                None
            };
            let list_model = Self::locations_to_store(location_results);
            self.location_results.set_model(Some(&list_model));
            self.location_results.set_visible(true);

            match results_count {
                1 => {
                    if let Some(first) = first_result {
                        // Force change trigger
                        self.location_results.set_active_id(Some(&first.location));
                        self.request_weather(first);
                    } 
                }, 
                _ => {
                    self.location_results.popup();
                },
            }
        } else {
            if let Err(err) = self.get_sender().send(WeatherUpdate::Location(None)) {
                println!("Unable to send WeatherUpdate::Location(None): {}", err);
            }
        }
    }

    fn update_location(&mut self, location: Option<String>) {
        if let Some(location) = location {
            self.location.set_visible(true);
            self.location_search.set_visible(false);
            self.location_results.set_visible(false);
            self.location_search_button.set_visible(false);
            self.refresh_button.set_visible(true);
            self.daily.set_visible(true);
            self.location.set_text(&location);
            self.set_stack_components_visible(true);
        } else {
            self.location.set_text("");
            self.location.set_visible(false);
            self.location_search.set_visible(true);    
            self.location_search_button.set_visible(true);
            self.refresh_button.set_visible(false);
            self.daily.set_visible(false);
            self.location_search.set_text("");
            self.update_current_weather(None);
            self.update_daily_weather(None);
            self.update_hourly_weather(None);
            self.set_stack_components_visible(false);
        }
    }

    fn set_stack_components_visible(&self, visible: bool) {
        self.stack_buttons_container.set_visible(visible);

        if let Ok(stack_view) = self.stack_view.try_lock() {
            stack_view.set_visible(visible);
        } else {
            println!("Unable to update stack view, mutex could not be locked");
        }
    }

    fn save_preferences(&self, preferences: &WeatherPreferences) {
        preferences.save_config();
    }

    fn update_units(&mut self, units: Units) {
        if let Some(prefs) = &mut self.preferences {
            prefs.units = units;
        }
    }

    fn get_units(&self) -> Units {
        if let Some(prefs) = &self.preferences {
            match prefs.units {
                Units::Metric => Units::Metric,
                Units::Imperial => Units::Imperial,
            }
        } else {
            Units::Metric
        }
    }
}
