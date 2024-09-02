use pyo3::prelude::*;
use pyo3::types::{PyModule, PyAny, PyList, PyTuple};
use std::path::{Path, PathBuf};
use std::{env, fs};
use std::fs::File;
use lazy_static::lazy_static;
use serde_json::Value;
use std::sync::{Arc, Mutex};
use shared_memory::{Shmem, ShmemConf};
// use tokio::io::AsyncWriteExt;
use shared_memory::ShmemError;
use tokio::signal::unix::{signal, SignalKind};
use std::io::{Write, Result};

const FILE_PATH: &str = "shared_data.txt";

// fn write_to_shared_memory(shmem: &mut Shmem, message: &str) {
//     let memSlice = unsafe {
//         shmem.as_slice()
//     };
//
//     let nullTerminatorPosition = memSlice.iter().position(|&x| x == 0).unwrap_or(memSlice.len());
//     String::from_utf8_lossy(&memSlice[..nullTerminatorPosition]).to_string()
// }

fn write_to_shared_memory(shmem: &mut Shmem, message: &str) {
    unsafe {
        let mem_slice = shmem.as_slice_mut();
        // Ensure that the message fits within the shared memory segment
        if message.len() <= mem_slice.len() {
            mem_slice[..message.len()].copy_from_slice(message.as_bytes());
            println!("shared memory에 작성한 데이터: {:?}", &mem_slice[..message.len()]);
        } else {
            eprintln!("Message is too large to fit in the shared memory segment.");
        }
    }
}

// fn write_to_shared_memory(shmem: &mut Shmem, message: &str) {
// //     let mem_slice = unsafe {
// //         shmem.as_slice_mut();
// //     }
// // //     mem_slice[..message.len()].copy_from_slice(message.as_bytes());
// //     if message.len() <= mem_slice.len() {
// //         mem_slice[..message.len()].copy_from_slice(message.as_bytes());
// //     } else {
// //         eprintln!("Message is too large to fit in the shared memory segment.");
// //     }
//     match shmem.as_slice_mut() {
//         Ok(mem_slice) => {
//             // Ensure that the message fits within the shared memory segment
//             if message.len() <= mem_slice.len() {
//                 mem_slice[..message.len()].copy_from_slice(message.as_bytes());
//             } else {
//                 eprintln!("Message is too large to fit in the shared memory segment.");
//             }
//         }
//         Err(e) => {
//             eprintln!("Failed to access shared memory as mutable slice: {}", e);
//         }
//     }
// }

// unsafe fn write_to_shared_memory(message: &str) {
//     let mut shmem = ShmemConf::new().size(4096).os_id("rust_shared_memory").create().expect("Failed to create shared memory");
//     println!("Shared memory created with ID: {:?}", shmem.as_ptr());
//     let mem_slice = shmem.as_slice_mut();
//     mem_slice[..message.len()].copy_from_slice(message.as_bytes());
// }

fn parse_json_to_string_args(py: Python, json_str: &str) -> Vec<PyObject> {
    let parsed_json: Value = serde_json::from_str(json_str).expect("Failed to parse JSON");

    let mut args: Vec<PyObject> = Vec::new();

    if let Value::Array(params) = parsed_json {
        for param in params {
            let py_object = match param {
                Value::String(s) => s.to_object(py),
                Value::Number(n) => n.to_string().to_object(py),
                Value::Bool(b) => b.to_string().to_object(py),
                Value::Null => "null".to_string().to_object(py),
                _ => {
                    eprintln!("Unsupported parameter type");
                    continue;
                }
            };
            args.push(py_object);
        }
    } else {
        eprintln!("Expected a JSON array");
    }

    args
}

fn add_subdirectories_to_pythonpath(root_path: &Path) -> String {
    let mut paths = vec![root_path.to_path_buf()];  // 루트 디렉토리 추가

    if let Ok(entries) = fs::read_dir(root_path) {
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                paths.push(path.clone());
                let sub_paths = add_subdirectories_to_pythonpath(&path);  // 재귀적으로 하위 디렉토리 탐색
                paths.push(PathBuf::from(sub_paths));
            }
        }
    }

    // 경로를 콜론(:)으로 구분하여 하나의 문자열로 연결
    paths.iter().map(|p| p.display().to_string()).collect::<Vec<String>>().join(":")
}

// TODO: 일단 구동만 되게 만들 것이므로 구조는 개 주고 만든다.
#[tokio::main]
async fn main() -> PyResult<()> {
    // let mut shmem = ShmemConf::new()
    //     .size(4096)
    //     .os_id("rust_shared_memory")
    //     .create()
    //     .or_else(|err| match err {
    //         ShmemError::MappingIdExists => ShmemConf::new().os_id("rust_shared_memory").open(),
    //         _ => Err(err),
    //     })
    //     .expect("Failed to create or open shared memory");

    // TODO: 향후 Chunk 단위 비동기 Shared Memory 송수신 처리가 완료 되면 활성화
    // let mut shmem = ShmemConf::new()
    //     // mac은 요걸로 요청해야함
    //     // .os_id("/rust_shared_memory")
    //     .os_id("rust_shared_memory")
    //     .open()
    //     .expect("Failed to open shared memory");

    match env::current_dir() {
        Ok(path) => println!("현재 작업 디렉토리: {}", path.display()),
        Err(e) => println!("현재 작업 디렉토리 획득 실패: {}", e),
    }

    match env::current_exe() {
        Ok(path) => println!("현재 구동 디렉토리: {}", path.display()),
        Err(e) => println!("현재 구동 디렉토리 획득 실패: {}", e),
    }

    let argumentList: Vec<String> = env::args().collect();
    println!("Received argumentList: {:?}", argumentList);

    if argumentList.len() == 5 {
        println!("사용 방법이 잘못 되었습니다 -> 모듈 전체 경로, 베이스 모듈 이름, 클래스명, 함수명, 파라미터_리스트")
    }

    let fullPackageName = &argumentList[1];
    let basePackageName = &argumentList[2];
    let className = &argumentList[3];
    let functionName = &argumentList[4];

    // let jsonizedParameterList = env::args().nth(5).expect("파라미터 리스트는 JSON 타입임");
    // let parameterList: Value = serde_json::from_str(&jsonizedParameterList).expect("JSON 파싱 실패");

    // let mut argumentVector: Vec<Value> = Vec::new();

    // match parameterList {
    //     Value::Array(params) => {
    //         for param in params {
    //             argumentVector.push(param);
    //         }
    //     },
    //     _ => eprintln!("파라미터가 Array(List) 스타일이 아닙니다!"),
    // }

    println!("fullPackageName: {}", fullPackageName);
    println!("basePackageName: {}", basePackageName);
    println!("className: {}", className);
    println!("functionName: {}", functionName);
    // println!("parameterList: {}", parameterList);

    // for argument in &argumentVector {
    //     println!("Vec Element: {:?}", argument);
    // }
    //
    // let argumentCount = argumentVector.len();

    // TOP_DIR 경로를 가져옴
    let binding = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let top_dir = binding.parent().unwrap();
    println!("top_dir: {}", top_dir.display());

    // .env 파일 경로 설정 및 로드
    let env_path = top_dir.join(".env");
    dotenv::from_path(env_path).expect(".env file not found");
    let openai_api_key = env::var("OPENAI_API_KEY").expect("OPENAI_API_KEY not set in .env file");

    // PYTHONPATH 설정
    // let openai_api_test_path = top_dir.join("openai_api_test");
    let base_package_name: &str = &basePackageName;
    let openai_api_test_path = top_dir.join(base_package_name);
    println!("openai_api_test_path: {}", openai_api_test_path.display());

    let pythonpath = add_subdirectories_to_pythonpath(&openai_api_test_path);
    println!("pythonpath: {}", pythonpath);
    env::set_var("PYTHONPATH", &pythonpath);

    let param_count = env::args().len() - 5;  // 첫 5개는 다른 인수들 (binary, fullPackagePath 등)
    let mut parameters: Vec<String> = Vec::new();

    for i in 0..param_count {
        parameters.push(env::args().nth(5 + i).unwrap());
    }

    // sys.path 업데이트
    Python::with_gil(|py| {
        let path = py.import("os")?.getattr("path")?;
        // TODO: 홀로 실행하냐 연동해서 실행하냐에 따라 자동으로 분류되게 만들어야함 (일단 그냥 감)
        //         let abspath = path.call_method1("abspath", ("..",))?;
        let abspath = path.call_method1("abspath", ("",))?;

        let sys = PyModule::import(py, "sys")?;
        let sys_path: &PyList = sys.getattr("path")?.downcast()?;
        sys_path.insert(0, abspath)?;

        println!("sys.path after insertion: {:?}", sys_path);

        Ok::<(), PyErr>(())
    })?;

    let full_package_name: &str = &fullPackageName;
    // 비동기 작업을 실행하기 위해 Python 코루틴을 Future로 변환
    let coroutine = Python::with_gil(|py| -> PyResult<Py<PyAny>> {
        let args: Vec<PyObject> = parameters.iter()
            .map(|p| p.to_object(py))
            .collect();

        // let openai_api_test_service_impl = PyModule::import(py, "openai_api_test.service.openai_api_test_service_impl")
        //     .expect("Failed to import module");
        let openai_api_test_service_impl = PyModule::import(py, full_package_name)
            .expect("Failed to import module");

        // OpenaiApiTestServiceImpl 클래스 가져오기
        // let service_impl_class = openai_api_test_service_impl.getattr("OpenaiApiTestServiceImpl")?;
        let class_name: &str = &className;
        let service_impl_class = openai_api_test_service_impl.getattr(class_name)?;

        // OpenaiApiTestServiceImpl.getInstance() 호출하여 싱글톤 인스턴스 얻기
        let service_instance = service_impl_class.call_method0("getInstance")?;
        println!("Success Singleton getInstance()");

        // letsChat 메서드를 호출하여 코루틴을 반환
        // let coroutine = service_instance.call_method1("letsChat", ("Hello from Rust!",))?;
        let function_name: &str = &functionName;
        let args_tuple = PyTuple::new(py, &args);
        let coroutine = service_instance.call_method1(function_name, args_tuple)?;
        // let coroutine = service_instance.call_method1(function_name, ("Hello from Rust!",))?;
        Ok(coroutine.into())
    })?;

    // 코루틴을 Future로 변환하고 실행하여 Python 객체를 반환
    let result: PyResult<Py<PyAny>> = Python::with_gil(|py| {
        let asyncio = py.import("asyncio")?;
        let result = asyncio.call_method1("run", (coroutine,))?;
        Ok(result.into())
    });

    // 결과를 처리하고 출력
    if let Ok(ref result) = result {
        let message = Python::with_gil(|py| -> PyResult<String> {
            let message: String = result.as_ref(py).get_item("message")?.extract()?;
            println!("Result from Python: {}", message);
            Ok(message)
        })?;

        // 공유 메모리에 메시지 작성
        //         unsafe { write_to_shared_memory(&message); }

        // TODO: 향후 Chunk 단위 비동기 Shared Memory 송수신 처리가 완료 되면 활성화
        // write_to_shared_memory(&mut shmem, &message);

        let mut file = File::create(FILE_PATH)?;
        file.write_all(message.as_bytes());

        println!("whatWeHaveToGetData:{}", message);

        std::process::exit(0)
    } else {
        eprintln!("Failed to execute Python coroutine.");
        std::process::exit(1)
    }
}
