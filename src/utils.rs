/// Sanitize a sensor name by replacing spaces with dashes
pub fn sanitize_sensor_name(name: String) -> String {
    name.replace(" ", "-")
}