@if(RAY)
import self::hwrt::cast_ray;
@else
import self::swrt::cast_ray;

@compute
@workgroup_size(1)
fn main() {
    var x = cast_ray();
}
